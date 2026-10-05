use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Clone, Debug)]
pub struct MeshCandidate {
    pub mesh: Mesh,
    pub format_version: u8,
    pub reconstruction_version: u8,
    pub cache_key: Option<AssetKey>,
    pub mask: MaskType,
}
#[derive(Clone, Debug, Default)]
pub struct Candidates {
    pub user_override: Option<MeshCandidate>,
    pub authored_override: Option<MeshCandidate>,
    pub embedded: Option<MeshCandidate>,
    pub generated_cache: Option<MeshCandidate>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionSource {
    Memory,
    UserOverride,
    AuthoredOverride,
    Embedded,
    GeneratedCache,
    Reconstructed,
}
#[derive(Clone, Debug)]
pub struct ResolvedMesh {
    pub candidate: MeshCandidate,
    pub source: ResolutionSource,
    pub rejected: Vec<(ResolutionSource, &'static str)>,
}
struct Entry {
    base: AssetKey,
    candidate: MeshCandidate,
    rejected: Vec<(ResolutionSource, &'static str)>,
    bytes: usize,
}
pub struct MeshResolver {
    entries: BTreeMap<[u8; 32], Entry>,
    lru: VecDeque<[u8; 32]>,
    ignored: BTreeSet<[u8; 32]>,
    max_bytes: usize,
    max_entries: usize,
    bytes: usize,
}
impl MeshResolver {
    pub fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            lru: VecDeque::new(),
            ignored: BTreeSet::new(),
            max_bytes,
            max_entries,
            bytes: 0,
        }
    }
    pub fn resolve(
        &mut self,
        key: AssetKey,
        candidates: &Candidates,
        regenerate: impl FnOnce() -> Result<MeshCandidate, Error>,
    ) -> Result<ResolvedMesh, Error> {
        let memory_key = effective_key(key, candidates, self.ignored.contains(&key.0));
        if let Some(entry) = self.entries.get(&memory_key) {
            let out = ResolvedMesh {
                candidate: entry.candidate.clone(),
                source: ResolutionSource::Memory,
                rejected: entry.rejected.clone(),
            };
            self.lru.retain(|k| *k != memory_key);
            self.lru.push_back(memory_key);
            return Ok(out);
        }
        let sources = [
            (
                ResolutionSource::UserOverride,
                candidates.user_override.as_ref(),
            ),
            (
                ResolutionSource::AuthoredOverride,
                candidates.authored_override.as_ref(),
            ),
            (ResolutionSource::Embedded, candidates.embedded.as_ref()),
            (
                ResolutionSource::GeneratedCache,
                if self.ignored.contains(&key.0) {
                    None
                } else {
                    candidates.generated_cache.as_ref()
                },
            ),
        ];
        let mut rejected = Vec::new();
        let mut selected = None;
        for (source, mesh) in sources {
            if let Some(mesh) = mesh {
                let check = validate_candidate(mesh).and_then(|()| {
                    if source == ResolutionSource::GeneratedCache
                        && (mesh.cache_key != Some(key) || mesh.reconstruction_version < 2)
                    {
                        Err("generated identity/version mismatch")
                    } else {
                        Ok(())
                    }
                });
                match check {
                    Ok(()) => {
                        selected = Some((source, mesh.clone()));
                        break;
                    }
                    Err(reason) => rejected.push((source, reason)),
                }
            }
        }
        let (source, candidate) = if let Some(found) = selected {
            found
        } else {
            let mesh = regenerate()?;
            validate_candidate(&mesh).map_err(Error::InvalidInput)?;
            (ResolutionSource::Reconstructed, mesh)
        };
        let bytes = candidate.mesh.vertices.len() * std::mem::size_of::<Vertex>()
            + candidate.mesh.indices.len() * 4;
        if self.max_entries > 0 && bytes <= self.max_bytes {
            while !self.entries.is_empty()
                && (self.entries.len() >= self.max_entries || self.bytes > self.max_bytes - bytes)
            {
                if let Some(old) = self.lru.pop_front() {
                    if let Some(entry) = self.entries.remove(&old) {
                        self.bytes -= entry.bytes;
                    }
                }
            }
            self.entries.insert(
                memory_key,
                Entry {
                    base: key,
                    candidate: candidate.clone(),
                    rejected: rejected.clone(),
                    bytes,
                },
            );
            self.lru.push_back(memory_key);
            self.bytes += bytes;
        }
        Ok(ResolvedMesh {
            candidate,
            source,
            rejected,
        })
    }
    /// Like legacy ClearCache: authored/user/embedded sources remain eligible.
    pub fn clear_generated(&mut self, key: AssetKey) {
        self.ignored.insert(key.0);
        let removed: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(k, v)| (v.base == key).then_some(*k))
            .collect();
        for k in removed {
            if let Some(e) = self.entries.remove(&k) {
                self.bytes -= e.bytes;
            }
            self.lru.retain(|v| *v != k);
        }
    }
    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }
}
fn validate_candidate(c: &MeshCandidate) -> Result<(), &'static str> {
    if !(1..=3).contains(&c.format_version) {
        return Err("FSOM format version");
    }
    if c.reconstruction_version != 0 && c.reconstruction_version < 2 {
        return Err("obsolete reconstruction version");
    }
    if c.mesh.vertices.is_empty() || c.mesh.indices.is_empty() {
        return Err("empty mesh");
    }
    c.mesh
        .validate(&RenderLimits::default())
        .map_err(|_| "corrupt mesh")
}
fn effective_key(key: AssetKey, c: &Candidates, ignore: bool) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"wonderland-mesh-resolution-v1\0");
    h.update(key.0);
    h.update([u8::from(ignore)]);
    for candidate in [
        &c.user_override,
        &c.authored_override,
        &c.embedded,
        &c.generated_cache,
    ] {
        match candidate {
            None => h.update([0]),
            Some(c) => {
                h.update([1, c.format_version, c.reconstruction_version, c.mask as u8]);
                if let Some(k) = c.cache_key {
                    h.update([1]);
                    h.update(k.0);
                } else {
                    h.update([0]);
                }
                if let Err(reason) = validate_candidate(c) {
                    h.update(reason.as_bytes());
                    continue;
                }
                h.update((c.mesh.vertices.len() as u64).to_le_bytes());
                h.update((c.mesh.indices.len() as u64).to_le_bytes());
                for v in &c.mesh.vertices {
                    for f in [
                        v.position.x,
                        v.position.y,
                        v.position.z,
                        v.normal.x,
                        v.normal.y,
                        v.normal.z,
                        v.uv.x,
                        v.uv.y,
                        v.color[0],
                        v.color[1],
                        v.color[2],
                        v.color[3],
                    ] {
                        h.update(f.to_bits().to_le_bytes());
                    }
                }
                for i in &c.mesh.indices {
                    h.update(i.to_le_bytes());
                }
            }
        }
    }
    h.finalize().into()
}
