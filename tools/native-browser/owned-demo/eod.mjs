// Private logical owned-session transport. This helper has no broadcast or
// raw-command capability. A socket write is never a delivery acknowledgement.
import assert from "node:assert/strict";
const decimal = (v) => typeof v === "string" && /^[1-9][0-9]{0,19}$/.test(v) && BigInt(v) <= 18446744073709551615n;
const fields = (v, keys) => {
  assert.ok(v && typeof v === "object" && !Array.isArray(v));
  assert.deepEqual(Object.keys(v).sort(), [...keys].sort());
};
export function privateEodRequest(query) {
  assert.ok(query && typeof query === "object");
  if (query.type === "native_eod_read") {
    fields(query, ["type"]);
    return {kind:"read"};
  }
  if (query.type === "native_eod_ui") {
    fields(query,["type","message"]);
    const m=query.message;
    fields(m,["schema_version","source_session_id","sequence","event","binary","payload_hex"]);
    assert.equal(m.schema_version,1);
    assert.ok(decimal(m.source_session_id) && decimal(m.sequence));
    assert.ok(typeof m.event === "string" && /^[\x20-\x7e]{1,64}$/.test(m.event));
    assert.equal(typeof m.binary,"boolean");
    assert.ok(typeof m.payload_hex === "string" && m.payload_hex.length<=8192 && /^(?:[0-9a-f]{2})*$/.test(m.payload_hex));
    return {kind:"prepare_eod_ui",extra:{message:m}};
  }
  fields(query,["type","source_session_id","output_id","sha256"]);
  assert.equal(query.type,"native_eod_ack");
  assert.ok(decimal(query.source_session_id) && decimal(query.output_id));
  assert.ok(typeof query.sha256 === "string" && /^[0-9a-f]{64}$/.test(query.sha256));
  return {kind:"prepare_eod_ack",extra:{source_session_id:query.source_session_id,output_id:query.output_id,sha256:query.sha256}};
}
export async function authenticatedEod(query, entry, {revalidate,prepare,commit,read,send}) {
  const request=privateEodRequest(query);
  await revalidate(entry);
  if(request.kind!=="read") {
    const result=await commit(await prepare(request.kind,{...request.extra,control:entry.control}));
    assert.equal(result.kind,"committed");
  }
  const output=await read(entry.control);
  assert.equal(output.kind,"private_eod_outputs");
  send({type:"native_eod_outputs",messages:output.messages});
}
