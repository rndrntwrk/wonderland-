//! W15.1: native-only MySQL/MariaDB ownership claims, independent of rendering.
//! Actor/session authorization happens before this trusted-server repository.
#![forbid(unsafe_code)]
mod policy;
use policy::{PolicyError, next_epoch, require_live};
use sqlx::{MySql, MySqlConnection, MySqlPool, Row, Transaction};
use std::{fmt, future::Future, pin::Pin};
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Database(sqlx::Error),
    Invalid(&'static str),
    Busy,
    Stale,
    EpochExhausted,
    Corrupt,
}
impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
impl From<PolicyError> for Error {
    fn from(e: PolicyError) -> Self {
        match e {
            PolicyError::Stale => Self::Stale,
            PolicyError::Exhausted => Self::EpochExhausted,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(e) => write!(f, "claim database: {e}"),
            other => write!(f, "claim rejected: {other:?}"),
        }
    }
}
impl std::error::Error for Error {}

/// Storage-local key, not a replacement for the shared F contract IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimKey {
    kind: u8,
    id: u64,
}
impl ClaimKey {
    pub fn lot(id: u64) -> Result<Self> {
        Self::new(1, id)
    }
    pub fn avatar(id: u64) -> Result<Self> {
        Self::new(2, id)
    }
    fn new(kind: u8, id: u64) -> Result<Self> {
        if id == 0 {
            Err(Error::Invalid("zero entity"))
        } else {
            Ok(Self { kind, id })
        }
    }
    pub fn kind(self) -> u8 {
        self.kind
    }
    pub fn id(self) -> u64 {
        self.id
    }
}
/// An operator-assigned unique process/actor incarnation nonce (e.g. UUID v4).
/// It is not a browser credential and must never come from a client payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerNonce([u8; 16]);
impl OwnerNonce {
    pub fn new(bytes: [u8; 16]) -> Result<Self> {
        if bytes == [0; 16] {
            Err(Error::Invalid("zero owner"))
        } else {
            Ok(Self(bytes))
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct LeaseMs(u32);
impl LeaseMs {
    pub fn new(ms: u32) -> Result<Self> {
        if !(1..=300_000).contains(&ms) {
            Err(Error::Invalid("lease outside 1..300000 ms"))
        } else {
            Ok(Self(ms))
        }
    }
}
/// Opaque native credential. No Deserialize or public constructor: stale data
/// is checked against the database, not a user-provided clock or expiry field.
#[derive(Clone, Debug)]
pub struct Claim {
    key: ClaimKey,
    owner: OwnerNonce,
    epoch: u64,
    lease_until_ms: u64,
}
impl Claim {
    pub fn key(&self) -> ClaimKey {
        self.key
    }
    pub fn owner(&self) -> OwnerNonce {
        self.owner
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn lease_until_ms(&self) -> u64 {
        self.lease_until_ms
    }
}
#[derive(Clone)]
pub struct ClaimRepository {
    pool: MySqlPool,
}
pub type FencedWork<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;
struct Stored {
    owner: Option<[u8; 16]>,
    epoch: u64,
    until: u64,
}
const CLOCK: &str = "SELECT CAST(UNIX_TIMESTAMP(CURRENT_TIMESTAMP(3))*1000 AS UNSIGNED)";
impl ClaimRepository {
    pub async fn new(pool: MySqlPool) -> Result<Self> {
        validate_schema(&pool).await?;
        Ok(Self { pool })
    }
    pub fn pool(&self) -> &MySqlPool {
        &self.pool
    }
    /// Explicit additive operator migration. Does not run as part of acquisition.
    pub async fn migrate(pool: &MySqlPool) -> Result<()> {
        sqlx::query(include_str!("../migrations/0001_claims.sql"))
            .execute(pool)
            .await?;
        Ok(())
    }
    pub async fn acquire(&self, key: ClaimKey, owner: OwnerNonce, ttl: LeaseMs) -> Result<Claim> {
        let mut tx = self.pool.begin().await?;
        // Duplicate handling is an update, not INSERT IGNORE: data errors are not
        // swallowed. The unique InnoDB row serializes both creation and takeover.
        sqlx::query("INSERT INTO wonderland_claims (claim_kind,entity_id) VALUES (?,?) ON DUPLICATE KEY UPDATE entity_id=entity_id").bind(key.kind).bind(key.id).execute(&mut *tx).await?;
        let stored = lock(&mut tx, key).await?;
        let now = db_now(&mut tx).await?; // MUST be a new statement after lock acquisition.
        if stored.owner.is_some() && stored.until > now {
            return Err(Error::Busy);
        }
        let epoch = next_epoch(stored.epoch)?;
        let claim = Claim {
            key,
            owner,
            epoch,
            lease_until_ms: deadline(now, ttl)?,
        };
        save(&mut tx, &claim).await?;
        tx.commit().await?;
        Ok(claim)
    }
    pub async fn renew(&self, claim: &Claim, ttl: LeaseMs) -> Result<Claim> {
        let mut tx = self.pool.begin().await?;
        let now = check(&mut tx, claim).await?;
        let next = Claim {
            lease_until_ms: deadline(now, ttl)?,
            ..claim.clone()
        };
        save(&mut tx, &next).await?;
        tx.commit().await?;
        Ok(next)
    }
    pub async fn transfer(
        &self,
        claim: &Claim,
        new_owner: OwnerNonce,
        ttl: LeaseMs,
    ) -> Result<Claim> {
        if new_owner == claim.owner {
            return Err(Error::Invalid("handoff requires a new owner"));
        }
        let mut tx = self.pool.begin().await?;
        let now = check(&mut tx, claim).await?;
        let next = Claim {
            owner: new_owner,
            epoch: next_epoch(claim.epoch)?,
            lease_until_ms: deadline(now, ttl)?,
            ..claim.clone()
        };
        save(&mut tx, &next).await?;
        tx.commit().await?;
        Ok(next)
    }
    pub async fn release(&self, claim: &Claim) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        check(&mut tx, claim).await?;
        sqlx::query("UPDATE wonderland_claims SET owner_nonce=NULL,lease_until_ms=0 WHERE claim_kind=? AND entity_id=?").bind(claim.key.kind).bind(claim.key.id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(()) // Retain epoch forever: release never deletes the row.
    }
    /// Guard a trusted server-side DML operation using the SAME locked transaction.
    /// The callback must not execute DDL, transaction-control statements, external
    /// effects, or acquire another claim in an inconsistent lock order. Keep it
    /// bounded. W15.2 consumers provide stable operation IDs and outbox semantics.
    /// A separate preflight `is_owner()` followed by an unguarded write is unsafe.
    pub async fn fenced<T, F>(&self, claim: &Claim, work: F) -> Result<T>
    where
        T: Send,
        F: for<'a> FnOnce(&'a mut MySqlConnection) -> FencedWork<'a, T>,
    {
        let mut tx = self.pool.begin().await?;
        check(&mut tx, claim).await?;
        let result = work(&mut tx).await;
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                tx.rollback().await?;
                return Err(e);
            }
        };
        // Reject work that outlived expiry or altered its own credential. The row
        // remains locked through commit, so a new epoch cannot commit in between.
        if let Err(e) = check(&mut tx, claim).await {
            tx.rollback().await?;
            return Err(e);
        }
        tx.commit().await?;
        Ok(value)
    }
}
fn deadline(now: u64, ttl: LeaseMs) -> Result<u64> {
    now.checked_add(u64::from(ttl.0))
        .ok_or(Error::Invalid("clock overflow"))
}
async fn db_now(tx: &mut Transaction<'_, MySql>) -> Result<u64> {
    Ok(sqlx::query_scalar::<_, u64>(CLOCK)
        .fetch_one(&mut **tx)
        .await?)
}
async fn lock(tx: &mut Transaction<'_, MySql>, key: ClaimKey) -> Result<Stored> {
    let row=sqlx::query("SELECT owner_nonce,fencing_epoch,lease_until_ms FROM wonderland_claims WHERE claim_kind=? AND entity_id=? FOR UPDATE").bind(key.kind).bind(key.id).fetch_optional(&mut **tx).await?.ok_or(Error::Stale)?;
    let owner = row
        .try_get::<Option<Vec<u8>>, _>("owner_nonce")?
        .map(|v| v.try_into().map_err(|_| Error::Corrupt))
        .transpose()?;
    Ok(Stored {
        owner,
        epoch: row.try_get("fencing_epoch")?,
        until: row.try_get("lease_until_ms")?,
    })
}
async fn check(tx: &mut Transaction<'_, MySql>, claim: &Claim) -> Result<u64> {
    let stored = lock(tx, claim.key).await?;
    let now = db_now(tx).await?;
    require_live(
        stored.owner,
        stored.epoch,
        stored.until,
        claim.owner.0,
        claim.epoch,
        now,
    )?;
    Ok(now)
}
async fn save(tx: &mut Transaction<'_, MySql>, claim: &Claim) -> Result<()> {
    sqlx::query("UPDATE wonderland_claims SET owner_nonce=?,fencing_epoch=?,lease_until_ms=? WHERE claim_kind=? AND entity_id=?").bind(claim.owner.0.as_slice()).bind(claim.epoch).bind(claim.lease_until_ms).bind(claim.key.kind).bind(claim.key.id).execute(&mut **tx).await?;
    Ok(())
}
// Fail closed when an existing table cannot provide row-level transaction locks.
async fn validate_schema(pool: &MySqlPool) -> Result<()> {
    let engine:Option<String>=sqlx::query_scalar("SELECT ENGINE FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='wonderland_claims'").fetch_optional(pool).await?;
    if engine.as_deref() != Some("InnoDB") {
        return Err(Error::Corrupt);
    }
    let all:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='wonderland_claims'").fetch_one(pool).await?;
    let correct:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='wonderland_claims' AND ((COLUMN_NAME='claim_kind' AND DATA_TYPE='tinyint' AND COLUMN_TYPE LIKE '%unsigned%' AND IS_NULLABLE='NO') OR (COLUMN_NAME IN ('entity_id','fencing_epoch','lease_until_ms') AND DATA_TYPE='bigint' AND COLUMN_TYPE LIKE '%unsigned%' AND IS_NULLABLE='NO') OR (COLUMN_NAME='owner_nonce' AND DATA_TYPE='binary' AND CHARACTER_MAXIMUM_LENGTH=16 AND IS_NULLABLE='YES'))").fetch_one(pool).await?;
    let primary:Option<String>=sqlx::query_scalar("SELECT GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX SEPARATOR ',') FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='wonderland_claims' AND INDEX_NAME='PRIMARY'").fetch_one(pool).await?;
    if all != 5 || correct != 5 || primary.as_deref() != Some("claim_kind,entity_id") {
        return Err(Error::Corrupt);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_ids_owners_and_leases_fail_without_io() {
        assert!(ClaimKey::lot(0).is_err());
        assert!(ClaimKey::avatar(0).is_err());
        assert!(OwnerNonce::new([0; 16]).is_err());
        assert!(LeaseMs::new(0).is_err());
        assert!(LeaseMs::new(300_001).is_err());
        assert!(LeaseMs::new(300_000).is_ok());
    }
    #[test]
    fn database_widths_and_namespaces_are_distinct() {
        let k = ClaimKey::lot(u64::MAX).unwrap();
        assert_eq!(k.id(), u64::MAX);
        assert_ne!(k, ClaimKey::avatar(u64::MAX).unwrap());
    }
    #[test]
    fn deadlines_never_wrap() {
        assert!(deadline(u64::MAX, LeaseMs::new(1).unwrap()).is_err());
    }
}
