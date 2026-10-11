-- Additive W15.1 storage only. Never delete rows: retained epochs prevent ABA.
-- Apply with an operator/migration credential, not the ordinary runtime user.
CREATE TABLE IF NOT EXISTS wonderland_claims (
    claim_kind TINYINT UNSIGNED NOT NULL,
    entity_id BIGINT UNSIGNED NOT NULL,
    owner_nonce BINARY(16) NULL,
    fencing_epoch BIGINT UNSIGNED NOT NULL DEFAULT 0,
    lease_until_ms BIGINT UNSIGNED NOT NULL DEFAULT 0,
    PRIMARY KEY (claim_kind, entity_id),
    CONSTRAINT wonderland_claim_kind CHECK (claim_kind IN (1, 2)),
    CONSTRAINT wonderland_claim_entity CHECK (entity_id > 0)
) ENGINE=InnoDB
