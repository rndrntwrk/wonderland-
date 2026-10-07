//! Real database tests. Missing DATABASE_URL is a failure, never a skipped pass.
use sqlx::{Row, mysql::MySqlPoolOptions};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{Barrier, Notify};
use wonderland_persistence::{ClaimKey, ClaimRepository, Error, LeaseMs, OwnerNonce};
static IDS: AtomicU64 = AtomicU64::new(100);
async fn repo() -> ClaimRepository {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL is mandatory: tests require actual MySQL/MariaDB");
    let p = MySqlPoolOptions::new()
        .max_connections(24)
        .connect(&url)
        .await
        .unwrap();
    let name: String = sqlx::query_scalar("SELECT DATABASE()")
        .fetch_one(&p)
        .await
        .unwrap();
    assert!(
        name.ends_with("_claimstest"),
        "Refusing destructive tests outside a dedicated *_claimstest database"
    );
    ClaimRepository::migrate(&p).await.unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS claim_test_writes (id BIGINT UNSIGNED PRIMARY KEY, value BIGINT UNSIGNED NOT NULL) ENGINE=InnoDB").execute(&p).await.unwrap();
    ClaimRepository::new(p).await.unwrap()
}
fn key() -> ClaimKey {
    ClaimKey::lot(IDS.fetch_add(1, Ordering::SeqCst)).unwrap()
}
fn owner(n: u8) -> OwnerNonce {
    OwnerNonce::new([n; 16]).unwrap()
}
fn lease() -> LeaseMs {
    LeaseMs::new(30_000).unwrap()
}
async fn expired(r: &ClaimRepository, k: ClaimKey) {
    sqlx::query("UPDATE wonderland_claims SET lease_until_ms=1 WHERE claim_kind=? AND entity_id=?")
        .bind(k.kind())
        .bind(k.id())
        .execute(r.pool())
        .await
        .unwrap();
}
#[tokio::test]
async fn mapping_and_retained_epoch() {
    let r = repo().await;
    ClaimRepository::migrate(r.pool()).await.unwrap();
    let k = ClaimKey::lot(u64::MAX).unwrap();
    let a = ClaimKey::avatar(u64::MAX).unwrap();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    let av = r.acquire(a, owner(2), lease()).await.unwrap();
    assert_eq!(c.epoch(), 1);
    assert_eq!(av.epoch(), 1);
    r.release(&c).await.unwrap();
    let newer = r.acquire(k, owner(1), lease()).await.unwrap();
    assert_eq!(newer.epoch(), 2);
    assert!(matches!(r.release(&c).await, Err(Error::Stale)));
}
#[tokio::test]
async fn competing_acquisitions_have_one_winner() {
    let r = repo().await;
    let k = key();
    let barrier = Arc::new(Barrier::new(16));
    let mut tasks = vec![];
    for n in 1..=16 {
        let r = r.clone();
        let b = barrier.clone();
        tasks.push(tokio::spawn(async move {
            b.wait().await;
            r.acquire(k, owner(n), lease()).await
        }));
    }
    let (mut winners, mut busy) = (0, 0);
    for task in tasks {
        match task.await.unwrap() {
            Ok(_) => winners += 1,
            Err(Error::Busy) => busy += 1,
            Err(e) => panic!("unexpected database error: {e}"),
        }
    }
    assert_eq!((winners, busy), (1, 15));
}
#[tokio::test]
async fn transfer_fences_old_credentials_and_renew_preserves_epoch() {
    let r = repo().await;
    let k = key();
    let c = r
        .acquire(k, owner(1), LeaseMs::new(5_000).unwrap())
        .await
        .unwrap();
    let renewed = r.renew(&c, lease()).await.unwrap();
    assert_eq!(renewed.epoch(), c.epoch());
    assert!(renewed.lease_until_ms() > c.lease_until_ms());
    let moved = r.transfer(&renewed, owner(2), lease()).await.unwrap();
    assert_eq!(moved.epoch(), 2);
    assert!(matches!(r.renew(&c, lease()).await, Err(Error::Stale)));
    assert!(matches!(r.release(&c).await, Err(Error::Stale)));
    assert!(matches!(
        r.fenced(&c, |_| Box::pin(async {
            panic!("stale callback must never execute");
            #[allow(unreachable_code)]
            Ok(())
        }))
        .await,
        Err(Error::Stale)
    ));
    r.fenced(&moved, |_| Box::pin(async { Ok(()) }))
        .await
        .unwrap();
}
#[tokio::test]
async fn expiry_cannot_be_renewed_and_reacquire_advances_epoch() {
    let r = repo().await;
    let k = key();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    expired(&r, k).await;
    assert!(matches!(r.renew(&c, lease()).await, Err(Error::Stale)));
    let next = r.acquire(k, owner(2), lease()).await.unwrap();
    assert_eq!(next.epoch(), c.epoch() + 1);
}
#[tokio::test]
async fn callback_failure_rolls_back_business_write() {
    let r = repo().await;
    let k = key();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    let id = k.id();
    let result = r
        .fenced(&c, move |conn| {
            Box::pin(async move {
                sqlx::query("INSERT INTO claim_test_writes (id,value) VALUES (?,1)")
                    .bind(id)
                    .execute(conn)
                    .await?;
                Err::<(), _>(Error::Invalid("simulated application failure"))
            })
        })
        .await;
    assert!(result.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM claim_test_writes WHERE id=?")
        .bind(id)
        .fetch_one(r.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn current_fenced_write_commits_and_new_repository_observes_claim() {
    let r = repo().await;
    let k = key();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    let id = k.id();
    r.fenced(&c, move |conn| {
        Box::pin(async move {
            sqlx::query("INSERT INTO claim_test_writes (id,value) VALUES (?,7)")
                .bind(id)
                .execute(conn)
                .await?;
            Ok(())
        })
    })
    .await
    .unwrap();
    let new_pool = MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let next = ClaimRepository::new(new_pool).await.unwrap();
    assert!(matches!(
        next.acquire(k, owner(2), lease()).await,
        Err(Error::Busy)
    ));
    let value: u64 = sqlx::query_scalar("SELECT value FROM claim_test_writes WHERE id=?")
        .bind(id)
        .fetch_one(next.pool())
        .await
        .unwrap();
    assert_eq!(value, 7);
}
#[tokio::test]
async fn handoff_waits_for_fenced_transaction_then_rejects_old_writer() {
    let r = repo().await;
    let k = key();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (rr, cc, ee, go) = (r.clone(), c.clone(), entered.clone(), release.clone());
    let writer = tokio::spawn(async move {
        rr.fenced(&cc, move |conn| {
            Box::pin(async move {
                sqlx::query("INSERT INTO claim_test_writes (id,value) VALUES (?,8)")
                    .bind(k.id())
                    .execute(conn)
                    .await?;
                ee.notify_one();
                go.notified().await;
                Ok(())
            })
        })
        .await
    });
    entered.notified().await;
    let (rr, cc) = (r.clone(), c.clone());
    let mover = tokio::spawn(async move { rr.transfer(&cc, owner(2), lease()).await });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(
        !mover.is_finished(),
        "handoff crossed a still-locked fenced write"
    );
    release.notify_one();
    writer.await.unwrap().unwrap();
    let moved = mover.await.unwrap().unwrap();
    assert_eq!(moved.epoch(), 2);
    assert!(matches!(
        r.fenced(&c, |_| Box::pin(async { Ok(()) })).await,
        Err(Error::Stale)
    ));
}
#[tokio::test]
async fn expiry_during_callback_rolls_back_before_commit() {
    let r = repo().await;
    let k = key();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    let id = k.id();
    let result=r.fenced(&c,move|conn|Box::pin(async move {
  sqlx::query("INSERT INTO claim_test_writes (id,value) VALUES (?,9)").bind(id).execute(&mut *conn).await?;
  // Operator fault injection: force actual stored expiry during the transaction.
  sqlx::query("UPDATE wonderland_claims SET lease_until_ms=1 WHERE claim_kind=1 AND entity_id=?").bind(id).execute(conn).await?;Ok(())
 })).await;
    assert!(matches!(result, Err(Error::Stale)));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM claim_test_writes WHERE id=?")
        .bind(id)
        .fetch_one(r.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn database_time_is_read_after_waiting_for_row_lock() {
    let r = repo().await;
    let k = key();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    let mut held = r.pool().begin().await.unwrap();
    sqlx::query(
        "SELECT entity_id FROM wonderland_claims WHERE claim_kind=? AND entity_id=? FOR UPDATE",
    )
    .bind(k.kind())
    .bind(k.id())
    .fetch_one(&mut *held)
    .await
    .unwrap();
    sqlx::query("UPDATE wonderland_claims SET lease_until_ms=CAST(UNIX_TIMESTAMP(CURRENT_TIMESTAMP(3))*1000 AS UNSIGNED)+100 WHERE claim_kind=? AND entity_id=?").bind(k.kind()).bind(k.id()).execute(&mut *held).await.unwrap();
    let rr = r.clone();
    let pending = tokio::spawn(async move { rr.acquire(k, owner(2), lease()).await });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(!pending.is_finished());
    held.commit().await.unwrap();
    assert_eq!(pending.await.unwrap().unwrap().epoch(), c.epoch() + 1);
}
#[tokio::test]
async fn exhausted_epoch_fails_without_mutating_record() {
    let r = repo().await;
    let k = key();
    let c = r.acquire(k, owner(1), lease()).await.unwrap();
    r.release(&c).await.unwrap();
    sqlx::query("UPDATE wonderland_claims SET fencing_epoch=? WHERE claim_kind=? AND entity_id=?")
        .bind(u64::MAX)
        .bind(k.kind())
        .bind(k.id())
        .execute(r.pool())
        .await
        .unwrap();
    assert!(matches!(
        r.acquire(k, owner(2), lease()).await,
        Err(Error::EpochExhausted)
    ));
    let row=sqlx::query("SELECT owner_nonce,fencing_epoch FROM wonderland_claims WHERE claim_kind=? AND entity_id=?").bind(k.kind()).bind(k.id()).fetch_one(r.pool()).await.unwrap();
    assert!(row.get::<Option<Vec<u8>>, _>("owner_nonce").is_none());
    assert_eq!(row.get::<u64, _>("fencing_epoch"), u64::MAX);
}

#[tokio::test]
async fn refuses_non_transactional_schema() {
    let r = repo().await;
    sqlx::query("ALTER TABLE wonderland_claims ENGINE=MyISAM")
        .execute(r.pool())
        .await
        .unwrap();
    let result = ClaimRepository::new(r.pool().clone()).await;
    sqlx::query("ALTER TABLE wonderland_claims ENGINE=InnoDB")
        .execute(r.pool())
        .await
        .unwrap();
    assert!(matches!(result, Err(Error::Corrupt)));
}
