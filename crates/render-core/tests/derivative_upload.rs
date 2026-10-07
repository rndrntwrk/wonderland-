use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use wonderland_render_core::derivatives::{upload::*, *};
use wonderland_render_core::*;
struct Resource(Arc<AtomicUsize>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn cpu() -> (DerivativeQueue, DerivativeLease) {
    let frame = RenderFrame {
        stamp: FrameStamp {
            lot_id: 1,
            epoch: 1,
            tick: 1,
            architecture_revision: 1,
            content: AssetKey([1; 32]),
        },
        entities: vec![],
        selected: None,
    };
    let q = DerivativeQueue::new(Default::default(), Default::default()).unwrap();
    q.reset(1, 1).unwrap();
    q.admit_frame(frame.clone()).unwrap();
    let p = PreparedDerivative::new(
        DerivativeInput {
            frame,
            source_provenance: AssetKey([2; 32]),
            lighting: [LightingPass::DAY, LightingPass::NIGHT],
            materials: vec![],
            draws: vec![],
            output: DerivativeOutput::Thumbnail(ThumbnailRequest {
                width: 4,
                height: 4,
                clip_from_world: Mat4::IDENTITY,
                clear: [0; 4],
            }),
        },
        Default::default(),
    )
    .unwrap();
    let ticket = q.submit(p).unwrap();
    q.start_next().unwrap().unwrap().execute().unwrap();
    let lease = q.acquire(&ticket.key()).unwrap().unwrap();
    (q, lease)
}
#[test]
fn upload_ownership_moves_once_and_device_reset_rejects_stale_result() {
    let (_q, lease) = cpu();
    let cache = DerivativeGpuCache::new(UploadLimits::default());
    let drops = Arc::new(AtomicUsize::new(0));
    let job = cache.begin_upload(lease, 128).unwrap();
    assert_eq!(cache.stats().unwrap().pending_bytes, 128);
    cache.reset_device().unwrap();
    assert!(job.is_cancelled());
    assert_eq!(
        job.complete(Resource(drops.clone())).unwrap(),
        Completion::Stale
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(cache.stats().unwrap().total_bytes, 0);
}
#[test]
fn source_invalidation_retains_gpu_and_cpu_lease_bytes_until_last_consumer_drops() {
    let (q, lease) = cpu();
    let key = lease.artifact().key();
    let cache = DerivativeGpuCache::new(UploadLimits {
        max_bytes: 128,
        ..UploadLimits::default()
    });
    let drops = Arc::new(AtomicUsize::new(0));
    cache
        .begin_upload(lease, 128)
        .unwrap()
        .complete(Resource(drops.clone()))
        .unwrap();
    let gpu = cache.acquire(&key).unwrap().unwrap();
    q.reset(1, 1).unwrap();
    assert!(!gpu.is_current());
    assert!(cache.acquire(&key).unwrap().is_none());
    assert_eq!(cache.stats().unwrap().total_bytes, 128);
    assert_eq!(q.stats().unwrap().retired_entries, 1);
    let (_other, second) = cpu();
    assert!(cache.begin_upload(second, 128).is_err());
    drop(gpu);
    assert_eq!(cache.stats().unwrap().total_bytes, 0);
    assert_eq!(q.stats().unwrap().resident_bytes, 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
#[test]
fn dropped_upload_and_superseded_candidate_cannot_clear_the_new_attempt() {
    let (q, lease) = cpu();
    let key = lease.artifact().key();
    let cache = DerivativeGpuCache::new(UploadLimits::default());
    let drops = Arc::new(AtomicUsize::new(0));
    let old = cache.begin_upload(lease, 128).unwrap();
    let new = cache
        .begin_upload(q.acquire(&key).unwrap().unwrap(), 128)
        .unwrap();
    assert_eq!(
        old.complete(Resource(drops.clone())).unwrap(),
        Completion::Stale
    );
    assert!(!new.is_cancelled());
    assert_eq!(
        new.complete(Resource(drops.clone())).unwrap(),
        Completion::Installed
    );
    assert!(cache.acquire(&key).unwrap().is_some());
    assert_eq!(cache.stats().unwrap().total_bytes, 128);
    let abandoned = cache
        .begin_upload(q.acquire(&key).unwrap().unwrap(), 128)
        .unwrap();
    drop(abandoned);
    assert_eq!(cache.stats().unwrap().in_flight, 0);
    cache.reset_device().unwrap();
    assert_eq!(cache.stats().unwrap().total_bytes, 0);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}
#[test]
fn upload_reservation_rejects_underreported_texture_bytes_before_ownership() {
    let (_q, lease) = cpu();
    let cache = DerivativeGpuCache::<()>::new(UploadLimits::default());
    assert!(cache.begin_upload(lease, 127).is_err());
    assert_eq!(cache.stats().unwrap().total_bytes, 0);
}
