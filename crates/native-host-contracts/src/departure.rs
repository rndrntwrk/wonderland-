//! Strict private departure observations, not account-save or custody-release
//! authorization. JSON shapes match native-edge account/lot-save and lot/departures.
use crate::DecimalId;
use serde::{Deserialize, Serialize};

pub const SOURCE_LOT_SAVE_PIN: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
pub const SOURCE_LOT_SAVE_LIMIT: usize = 64 * 1024;
pub const SOURCE_RELATIONSHIP_LIMIT: usize = 256;
const DEPARTURE_EVIDENCE_LIMIT: usize = 2 * 1024 * 1024;
type Validation = Result<(), &'static str>;

// Decode the exact object before checking semantic invariants. Nullable fields
// use deserialize_with, intentionally without default: omission is not null.
macro_rules! strict_dto {
    ($(#[$meta:meta])* pub struct $name:ident { $($(#[$field_meta:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        pub struct $name { $(pub $field: $ty),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Raw { $($(#[$field_meta])* $field: $ty),* }
                struct ObjectVisitor;
                impl<'de> serde::de::Visitor<'de> for ObjectVisitor {
                    type Value = $name;
                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        formatter.write_str("an exact departure JSON object")
                    }
                    fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                        let raw = Raw::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                        let value = $name { $($field: raw.$field),* };
                        value.validate().map_err(serde::de::Error::custom)?;
                        Ok(value)
                    }
                }
                // Derived struct serde also accepts positional sequences. The
                // private JSON parser accepts records only, never those arrays.
                deserializer.deserialize_map(ObjectVisitor)
            }
        }
    };
}
fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
// serde's default externally tagged unit enums also accept object forms. The
// edge wire contract accepts strings only, so reject those alternate shapes.
macro_rules! string_enum {
    (pub enum $name:ident { $($variant:ident => $wire:literal),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                match String::deserialize(deserializer)?.as_str() {
                    $($wire => Ok(Self::$variant),)*
                    _ => Err(serde::de::Error::custom("invalid departure enum value")),
                }
            }
        }
    };
}
fn nonzero(value: DecimalId) -> Validation {
    if value.0 == 0 {
        Err("zero departure identity or observation")
    } else {
        Ok(())
    }
}
fn hash(value: &str) -> Validation {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err("invalid departure digest")
    }
}
fn operation(value: &str) -> Validation {
    if (16..=96).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        Ok(())
    } else {
        Err("invalid departure operation identity")
    }
}

// Canonical object-key ordering matches native-edge canonical(). All DTO keys
// and validated strings are ASCII and all numeric values are exact JS integers.
// Sort explicitly rather than depending on serde_json's preserve_order feature.
fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, &'static str> {
    fn write(value: &serde_json::Value, out: &mut String) {
        use serde_json::Value;
        match value {
            Value::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    write(value, out);
                }
                out.push(']');
            }
            Value::Object(values) => {
                out.push('{');
                let mut fields: Vec<_> = values.iter().collect();
                fields.sort_unstable_by_key(|(a, _)| *a);
                for (index, (key, value)) in fields.into_iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(key).expect("JSON object key"));
                    out.push(':');
                    write(value, out);
                }
                out.push('}');
            }
            _ => out.push_str(&value.to_string()),
        }
    }
    let value = serde_json::to_value(value).map_err(|_| "departure JSON encoding failed")?;
    let mut out = String::new();
    write(&value, &mut out);
    Ok(out.into_bytes())
}

strict_dto! {
    pub struct DepartureEntityRefV1 { object_id: u16, generation: u32 }
}
impl DepartureEntityRefV1 {
    pub fn validate(&self) -> Validation {
        if self.object_id == 0 || self.object_id > 32767 || self.generation == 0 {
            Err("invalid departure entity reference")
        } else {
            Ok(())
        }
    }
}
strict_dto! {
    pub struct DepartureIdentityV1 {
        authority_id: String,
        account_id: DecimalId,
        avatar_id: DecimalId,
        lot_id: DecimalId,
        control_generation: DecimalId,
        session_ref: String,
        admission_operation: String,
        reservation_digest: String,
        export_digest: String,
        principal: DecimalId,
        actor: DepartureEntityRefV1,
    }
}
impl DepartureIdentityV1 {
    pub fn validate(&self) -> Validation {
        for id in [
            self.account_id,
            self.avatar_id,
            self.lot_id,
            self.control_generation,
            self.principal,
        ] {
            nonzero(id)?;
        }
        if self.avatar_id.0 > 16777215 {
            return Err("avatar identity outside source range");
        }
        for value in [
            &self.authority_id,
            &self.session_ref,
            &self.reservation_digest,
            &self.export_digest,
        ] {
            hash(value)?;
        }
        operation(&self.admission_operation)?;
        self.actor.validate()
    }
}
/// Private deterministic operation identity, shared by leave/logout/expiry.
/// This neither fences control nor authenticates a browser request.
pub fn departure_operation_v1(identity: &DepartureIdentityV1) -> Result<String, &'static str> {
    identity.validate()?;
    let result = format!(
        "{}_{}_{}",
        identity.authority_id, identity.avatar_id.0, identity.control_generation.0
    );
    operation(&result)?;
    Ok(result)
}
strict_dto! {
    pub struct DepartureRequestV1 { schema_version: u32, identity: DepartureIdentityV1, departure_operation: String }
}
impl DepartureRequestV1 {
    pub fn validate(&self) -> Validation {
        if self.schema_version != 1 {
            return Err("unsupported departure request");
        }
        if self.departure_operation != departure_operation_v1(&self.identity)? {
            return Err("departure operation must be server-derived");
        }
        Ok(())
    }
}
strict_dto! {
    /// Exact private coordinator fence, matching native-edge lot/control.ts. This
    /// is an asserted custody fact on the private supervisor pipe, not a credential.
    pub struct ControlFenceV1 {
        version: u32,
        authority: String,
        avatar_id: DecimalId,
        generation: DecimalId,
        lot_id: DecimalId,
        request_digest: String,
    }
}
impl ControlFenceV1 {
    pub fn validate(&self) -> Validation {
        if self.version != 1 {
            return Err("unsupported control fence");
        }
        hash(&self.authority)?;
        hash(&self.request_digest)?;
        for id in [self.avatar_id, self.generation, self.lot_id] {
            nonzero(id)?;
        }
        if self.avatar_id.0 > 16777215 {
            return Err("avatar identity outside source range");
        }
        Ok(())
    }
    pub fn matches_departure(&self, departure: &DepartureRequestV1) -> Validation {
        self.validate()?;
        departure.validate()?;
        let identity = &departure.identity;
        if self.authority != identity.authority_id
            || self.avatar_id != identity.avatar_id
            || self.generation != identity.control_generation
            || self.lot_id != identity.lot_id
            || self.request_digest != identity.reservation_digest
        {
            return Err("departure fence identity mismatch");
        }
        Ok(())
    }
}
strict_dto! {
    pub struct SourceAvatarColumnsV1 {
        motive_data_hex: String,
        skilllock: u8,
        lock_body: u16,
        lock_charisma: u16,
        lock_cooking: u16,
        lock_creativity: u16,
        lock_logic: u16,
        lock_mechanical: u16,
        skill_cooking: u16,
        skill_charisma: u16,
        skill_mechanical: u16,
        skill_creativity: u16,
        skill_body: u16,
        skill_logic: u16,
        body: DecimalId,
        body_sleepwear: DecimalId,
        body_swimwear: DecimalId,
        body_current: DecimalId,
        current_job: u16,
        is_ghost: u16,
        ticker_death: u16,
        ticker_gardener: u16,
        ticker_maid: u16,
        ticker_repairman: u16,
    }
}
impl SourceAvatarColumnsV1 {
    pub fn validate(&self) -> Validation {
        hash(&self.motive_data_hex).map_err(|_| "invalid source motive bytes")?;
        if &self.motive_data_hex[44..48] != "0000" {
            return Err("source SleepState must be zero");
        }
        Ok(())
    }
}
strict_dto! {
    pub struct SourceJobRowV1 { job_type: u16, job_experience: u16, job_level: u16, job_sickdays: u16, job_statusflags: u16 }
}
impl SourceJobRowV1 {
    pub fn validate(&self) -> Validation {
        if self.job_type == 0 {
            Err("source current job must be nonzero")
        } else {
            Ok(())
        }
    }
}
strict_dto! {
    pub struct SourceRelationshipV1 { target: DecimalId, values: Vec<i16> }
}
impl SourceRelationshipV1 {
    pub fn validate(&self) -> Validation {
        if self.target.0 == 0 || self.target.0 > u64::from(u32::MAX) {
            return Err("relationship target outside source range");
        }
        if self.values.len() > SOURCE_RELATIONSHIP_LIMIT {
            return Err("source relationship value capacity");
        }
        Ok(())
    }
}
string_enum! { pub enum SourceCaptureReasonV1 { FirstLeaveTick => "first_leave_tick" } }
strict_dto! {
    pub struct SourceAvatarLotSaveV1 {
        schema_version: u32,
        source_pin: String,
        identity: DepartureIdentityV1,
        departure_operation: String,
        save_sequence: DecimalId,
        captured_tick: DecimalId,
        capture_reason: SourceCaptureReasonV1,
        avatar_columns: SourceAvatarColumnsV1,
        #[serde(deserialize_with = "required_nullable")]
        current_job_row: Option<SourceJobRowV1>,
        changed_relationships: Vec<SourceRelationshipV1>,
    }
}
impl SourceAvatarLotSaveV1 {
    pub fn validate(&self) -> Validation {
        if self.schema_version != 1
            || self.source_pin != SOURCE_LOT_SAVE_PIN
            || self.save_sequence.0 != 1
        {
            return Err("unsupported source lot-save profile");
        }
        self.identity.validate()?;
        operation(&self.departure_operation)?;
        nonzero(self.captured_tick)?;
        self.avatar_columns.validate()?;
        if let Some(job) = &self.current_job_row {
            job.validate()?;
            if job.job_type != self.avatar_columns.current_job {
                return Err("source current-job mismatch");
            }
        }
        if self.changed_relationships.len() > SOURCE_RELATIONSHIP_LIMIT {
            return Err("source relationship capacity");
        }
        let mut previous = 0;
        for relationship in &self.changed_relationships {
            relationship.validate()?;
            if relationship.target.0 <= previous {
                return Err("source relationship order or duplicate");
            }
            previous = relationship.target.0;
        }
        if canonical_bytes(self)?.len() > SOURCE_LOT_SAVE_LIMIT {
            return Err("source lot-save byte capacity");
        }
        Ok(())
    }
    /// Hash these exact bytes before attaching any committed origin envelope.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, &'static str> {
        self.validate()?;
        canonical_bytes(self)
    }
}
string_enum! { pub enum DepartureLifecycleStatusV1 { Leaving => "leaving", Blocked => "blocked", Detached => "detached" } }
strict_dto! {
    pub struct DepartureLifecycleV1 {
        observed_tick: DecimalId,
        status: DepartureLifecycleStatusV1,
        #[serde(deserialize_with = "required_nullable")]
        blocker: Option<String>,
    }
}
impl DepartureLifecycleV1 {
    pub fn validate(&self) -> Validation {
        nonzero(self.observed_tick)?;
        match (self.status, &self.blocker) {
            (DepartureLifecycleStatusV1::Blocked, Some(blocker))
                if (1..=96).contains(&blocker.len())
                    && blocker
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') =>
            {
                Ok(())
            }
            (DepartureLifecycleStatusV1::Leaving | DepartureLifecycleStatusV1::Detached, None) => {
                Ok(())
            }
            _ => Err("invalid departure lifecycle"),
        }
    }
}
strict_dto! {
    pub struct DepartureSaveObservationV1 { save_record: SourceAvatarLotSaveV1, save_sha256: String, event_ordinal: u16 }
}
impl DepartureSaveObservationV1 {
    pub fn validate(&self) -> Validation {
        self.save_record.validate()?;
        // Digest authentication belongs to the host/coordinator, not serde.
        hash(&self.save_sha256)
    }
}
string_enum! { pub enum DepartureRemovalReasonV1 { SourceLeave => "source_leave", ForcedCleanup => "forced_cleanup" } }
strict_dto! {
    pub struct DepartureDetachObservationV1 { detached_tick: DecimalId, removal_reason: DepartureRemovalReasonV1, event_ordinal: u16 }
}
impl DepartureDetachObservationV1 {
    pub fn validate(&self) -> Validation {
        nonzero(self.detached_tick)
    }
}
strict_dto! {
    pub struct DepartureEvidenceV1 {
        schema_version: u32,
        identity: DepartureIdentityV1,
        departure_operation: String,
        lifecycle: DepartureLifecycleV1,
        #[serde(deserialize_with = "required_nullable")]
        save: Option<DepartureSaveObservationV1>,
        #[serde(deserialize_with = "required_nullable")]
        detach: Option<DepartureDetachObservationV1>,
        entity_refs_after: Vec<DepartureEntityRefV1>,
    }
}
impl DepartureEvidenceV1 {
    pub fn validate(&self) -> Validation {
        DepartureRequestV1 {
            schema_version: self.schema_version,
            identity: self.identity.clone(),
            departure_operation: self.departure_operation.clone(),
        }
        .validate()?;
        self.lifecycle.validate()?;
        if let Some(save) = &self.save {
            save.validate()?;
            if save.save_record.identity != self.identity
                || save.save_record.departure_operation != self.departure_operation
            {
                return Err("departure save identity mismatch");
            }
            if save.save_record.captured_tick.0 > self.lifecycle.observed_tick.0 {
                return Err("departure lifecycle observation mismatch");
            }
        }
        if (self.lifecycle.status == DepartureLifecycleStatusV1::Detached) != self.detach.is_some()
        {
            return Err("departure lifecycle observation mismatch");
        }
        if let Some(detach) = &self.detach {
            detach.validate()?;
            if detach.detached_tick != self.lifecycle.observed_tick
                || self
                    .save
                    .as_ref()
                    .is_some_and(|save| save.event_ordinal >= detach.event_ordinal)
            {
                return Err("departure lifecycle observation mismatch");
            }
        }
        if self.entity_refs_after.len() > 32767 {
            return Err("departure entity projection capacity");
        }
        let mut previous = 0;
        for reference in &self.entity_refs_after {
            reference.validate()?;
            if reference.object_id <= previous {
                return Err("departure entity projection order or duplicate");
            }
            previous = reference.object_id;
        }
        if self.entity_refs_after.contains(&self.identity.actor) == self.detach.is_some() {
            return Err("departure entity presence mismatch");
        }
        if canonical_bytes(self)?.len() > DEPARTURE_EVIDENCE_LIMIT {
            return Err("departure evidence capacity");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn identity() -> Value {
        json!({"authority_id":"1".repeat(64),"account_id":"1","avatar_id":"2","lot_id":"3",
            "control_generation":"18446744073709551615","session_ref":"2".repeat(64),
            "admission_operation":"admission_producer_0001","reservation_digest":"3".repeat(64),
            "export_digest":"4".repeat(64),"principal":"5","actor":{"object_id":6,"generation":7}})
    }
    fn request() -> Value {
        json!({"schema_version":1,"identity":identity(),
            "departure_operation":format!("{}_2_18446744073709551615", "1".repeat(64))})
    }
    fn save() -> Value {
        json!({"schema_version":1,"source_pin":SOURCE_LOT_SAVE_PIN,"identity":identity(),
            "departure_operation":request()["departure_operation"],"save_sequence":"1","captured_tick":"11",
            "capture_reason":"first_leave_tick","current_job_row":null,"changed_relationships":[],
            "avatar_columns":{"motive_data_hex":"0".repeat(64),"skilllock":255,
                "lock_body":65535,"lock_charisma":0,"lock_cooking":0,"lock_creativity":0,"lock_logic":0,"lock_mechanical":0,
                "skill_cooking":0,"skill_charisma":0,"skill_mechanical":0,"skill_creativity":0,"skill_body":0,"skill_logic":0,
                "body":"18446744073709551615","body_sleepwear":"0","body_swimwear":"1","body_current":"2",
                "current_job":0,"is_ghost":65535,"ticker_death":0,"ticker_gardener":0,"ticker_maid":0,"ticker_repairman":0}})
    }
    fn evidence() -> Value {
        let mut v = request();
        v["lifecycle"] = json!({"observed_tick":"11","status":"leaving","blocker":null});
        v["save"] = json!({"save_record":save(),"save_sha256":"5".repeat(64),"event_ordinal":0});
        v["detach"] = Value::Null;
        v["entity_refs_after"] = json!([{"object_id":6,"generation":7}]);
        v
    }
    #[test]
    fn private_owned_drain_request_and_fence_are_exact_and_command_free() {
        let identity = identity();
        let fence = json!({"version":1,"authority":identity["authority_id"],"avatar_id":identity["avatar_id"],
            "generation":identity["control_generation"],"lot_id":identity["lot_id"],"request_digest":identity["reservation_digest"]});
        let parsed: ControlFenceV1 = serde_json::from_value(fence.clone()).unwrap();
        let departure: DepartureRequestV1 = serde_json::from_value(request()).unwrap();
        parsed.matches_departure(&departure).unwrap();
        let mut mismatched = parsed;
        mismatched.generation = DecimalId(1);
        assert!(mismatched.matches_departure(&departure).is_err());
        let raw = json!({"kind":"prepare_owned_drain","batch_id":"1",
            "binding":{"protocol":1,"source_pin":SOURCE_LOT_SAVE_PIN,"binding_sha256":"a".repeat(64),"lot_incarnation":"1"},
            "predecessor":{"lot_id":"3","authority_epoch":"1","completed_tick":"10","state_hash":"b".repeat(64)},
            "expected_control_revision":"18446744073709551615","departure":request(),"fence":fence});
        assert!(matches!(
            serde_json::from_value::<crate::RequestV1>(raw.clone()).unwrap(),
            crate::RequestV1::PrepareOwnedDrain { .. }
        ));
        for field in ["commands_hex", "portable_json", "save_record", "action_hex"] {
            let mut bad = raw.clone();
            bad[field] = json!("00");
            assert!(serde_json::from_value::<crate::RequestV1>(bad).is_err());
        }
        for (field, value) in [
            ("version", json!(2)),
            ("authority", json!("A".repeat(64))),
            ("avatar_id", json!("0")),
            ("generation", json!("01")),
            ("lot_id", json!(0)),
        ] {
            let mut bad = raw.clone();
            bad["fence"][field] = value;
            assert!(serde_json::from_value::<crate::RequestV1>(bad).is_err());
        }
        let mut bad = raw.clone();
        bad["fence"]["extra"] = json!(0);
        assert!(serde_json::from_value::<crate::RequestV1>(bad).is_err());
        let mut bad = raw;
        bad["fence"] = json!([1, "a".repeat(64), "2", "3", "4", "b".repeat(64)]);
        assert!(serde_json::from_value::<crate::RequestV1>(bad).is_err());
    }
    #[test]
    fn canonical_decimal_identity_and_server_derived_request_round_trip() {
        let raw = request();
        let parsed: DepartureRequestV1 = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), raw);
        assert_eq!(
            departure_operation_v1(&parsed.identity).unwrap(),
            parsed.departure_operation
        );
        for (field, value) in [
            ("account_id", json!(1)),
            ("avatar_id", json!("16777216")),
            ("control_generation", json!("18446744073709551616")),
            ("principal", json!("01")),
            ("lot_id", json!("0")),
            ("authority_id", json!("A".repeat(64))),
            ("admission_operation", json!("short")),
            ("session_ref", json!("z".repeat(64))),
        ] {
            let mut bad = raw.clone();
            bad["identity"][field] = value;
            assert!(
                serde_json::from_value::<DepartureRequestV1>(bad).is_err(),
                "{field}"
            );
        }
        for (field, value) in [
            ("object_id", json!(0)),
            ("object_id", json!(32768)),
            ("generation", json!(0)),
            ("generation", json!(4294967296_u64)),
        ] {
            let mut bad = raw.clone();
            bad["identity"]["actor"][field] = value;
            assert!(serde_json::from_value::<DepartureRequestV1>(bad).is_err());
        }
        let mut bad = raw;
        bad["departure_operation"] = json!("arbitrary_operation_0001");
        assert!(serde_json::from_value::<DepartureRequestV1>(bad).is_err());
    }
    #[test]
    fn source_save_requires_exact_fields_and_nullable_job_presence() {
        let raw = save();
        let parsed: SourceAvatarLotSaveV1 = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), raw);
        for path in [
            vec![],
            vec!["identity"],
            vec!["identity", "actor"],
            vec!["avatar_columns"],
        ] {
            let mut bad = raw.clone();
            let mut at = &mut bad;
            for key in path {
                at = &mut at[key];
            }
            at["permission"] = json!(4);
            assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(bad).is_err());
        }
        for field in raw.as_object().unwrap().keys() {
            let mut bad = raw.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<SourceAvatarLotSaveV1>(bad).is_err(),
                "missing {field}"
            );
        }
        for (field, value) in [
            ("schema_version", json!(2)),
            ("source_pin", json!("0".repeat(40))),
            ("save_sequence", json!("2")),
            ("captured_tick", json!("0")),
            ("capture_reason", json!("forced_cleanup")),
        ] {
            let mut bad = raw.clone();
            bad[field] = value;
            assert!(
                serde_json::from_value::<SourceAvatarLotSaveV1>(bad).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn source_columns_are_bounded_and_sleep_state_must_be_normalized() {
        for (field, value) in [
            ("skilllock", json!(256)),
            ("skill_body", json!(-1)),
            ("lock_body", json!(65536)),
            ("body", json!("01")),
            ("body_current", json!(1)),
            ("motive_data_hex", json!("A".repeat(64))),
            ("motive_data_hex", json!("0".repeat(63))),
            (
                "motive_data_hex",
                json!(format!("{}0001{}", "0".repeat(44), "0".repeat(16))),
            ),
        ] {
            let mut bad = save();
            bad["avatar_columns"][field] = value;
            assert!(
                serde_json::from_value::<SourceAvatarLotSaveV1>(bad).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn source_job_and_relationship_values_are_strict_and_sorted() {
        let mut raw = save();
        raw["current_job_row"] = json!({"job_type":1,"job_experience":65535,"job_level":0,"job_sickdays":0,"job_statusflags":0});
        assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(raw.clone()).is_err());
        raw["avatar_columns"]["current_job"] = json!(1);
        assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(raw.clone()).is_ok());
        raw["current_job_row"]["extra"] = json!(0);
        assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(raw).is_err());
        for rels in [
            json!([{"target":"2","values":[]},{"target":"1","values":[]}]),
            json!([{"target":"1","values":[]},{"target":"1","values":[]}]),
            json!([{"target":"0","values":[]}]),
            json!([{"target":"4294967296","values":[]}]),
            json!([{"target":"1","values":[32768]}]),
            json!([{"target":"1","values":[-32769]}]),
            json!([{"target":"1","values":[],"extra":0}]),
            json!([{"target":"1","values":vec![0;257]}]),
        ] {
            let mut bad = save();
            bad["changed_relationships"] = rels;
            assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(bad).is_err());
        }
        let mut valid = save();
        valid["changed_relationships"] = json!([{"target":"4294967295","values":[-32768,32767]}]);
        assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(valid).is_ok());
    }
    #[test]
    fn lifecycle_nullable_fields_and_detach_presence_are_exact() {
        let raw = evidence();
        let d: DepartureEvidenceV1 = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(d).unwrap(), raw);
        for field in ["save", "detach"] {
            let mut bad = raw.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<DepartureEvidenceV1>(bad).is_err());
        }
        let mut bad = raw.clone();
        bad["lifecycle"].as_object_mut().unwrap().remove("blocker");
        assert!(serde_json::from_value::<DepartureEvidenceV1>(bad).is_err());
        for blocker in [
            json!(null),
            json!(""),
            json!("Not permitted"),
            json!("a".repeat(97)),
        ] {
            let mut bad = raw.clone();
            bad["lifecycle"]["status"] = json!("blocked");
            bad["lifecycle"]["blocker"] = blocker;
            assert!(serde_json::from_value::<DepartureEvidenceV1>(bad).is_err());
        }
        let mut blocked = raw.clone();
        blocked["lifecycle"]["status"] = json!("blocked");
        blocked["lifecycle"]["blocker"] = json!("unsupported_blocking_kind");
        assert!(serde_json::from_value::<DepartureEvidenceV1>(blocked).is_ok());
        let mut detached = raw;
        detached["lifecycle"]["status"] = json!("detached");
        detached["detach"] =
            json!({"detached_tick":"11","removal_reason":"source_leave","event_ordinal":1});
        assert!(serde_json::from_value::<DepartureEvidenceV1>(detached.clone()).is_err());
        detached["entity_refs_after"] = json!([]);
        assert!(serde_json::from_value::<DepartureEvidenceV1>(detached.clone()).is_ok());
        detached["detach"]["removal_reason"] = json!("forced_cleanup");
        assert!(serde_json::from_value::<DepartureEvidenceV1>(detached.clone()).is_ok());
        detached["detach"]["event_ordinal"] = json!(0);
        assert!(serde_json::from_value::<DepartureEvidenceV1>(detached.clone()).is_err());
        detached["detach"]["event_ordinal"] = json!(1);
        detached["detach"]["detached_tick"] = json!("12");
        assert!(serde_json::from_value::<DepartureEvidenceV1>(detached).is_err());
    }
    #[test]
    fn evidence_rejects_mismatched_identity_tick_projection_and_digest_shape() {
        for mutation in 0..6 {
            let mut bad = evidence();
            match mutation {
                0 => bad["save"]["save_record"]["identity"]["account_id"] = json!("2"),
                1 => bad["save"]["save_record"]["captured_tick"] = json!("12"),
                2 => bad["entity_refs_after"] = json!([]),
                3 => {
                    bad["entity_refs_after"] =
                        json!([{"object_id":6,"generation":7},{"object_id":6,"generation":8}])
                }
                4 => bad["save"]["save_sha256"] = json!("A".repeat(64)),
                _ => bad["save"]["event_ordinal"] = json!(65536),
            }
            assert!(
                serde_json::from_value::<DepartureEvidenceV1>(bad).is_err(),
                "mutation {mutation}"
            );
        }
    }
    #[test]
    fn dto_objects_and_enum_strings_have_no_alternate_serde_wire_forms() {
        assert!(serde_json::from_value::<DepartureEntityRefV1>(json!([6, 7])).is_err());
        let mut raw = save();
        raw["identity"]["actor"] = json!([6, 7]);
        assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(raw).is_err());
        let mut raw = save();
        raw["capture_reason"] = json!({"first_leave_tick":null});
        assert!(serde_json::from_value::<SourceAvatarLotSaveV1>(raw).is_err());
        let mut raw = evidence();
        raw["lifecycle"]["status"] = json!({"leaving":null});
        assert!(serde_json::from_value::<DepartureEvidenceV1>(raw).is_err());
        assert!(
            serde_json::from_value::<DepartureRemovalReasonV1>(json!({"source_leave":null}))
                .is_err()
        );
    }
}
