// Public lot conversation only. This service does not fabricate VM chat ticks,
// speech bubbles, private channels or mail. Membership is checked by its caller
// in the same SQLite transaction as every read/write.
import { decimal, fields } from './contracts';
export function chatText(raw: unknown): string {
  if (typeof raw !== 'string' || !raw.trim() || raw.length > 200
    || /[\u0000-\u0008\u000b-\u001f\u007f]/.test(raw)) throw Error('invalid lot chat text');
  for (let i=0;i<raw.length;i++) {
    const c=raw.charCodeAt(i);
    if (c>=0xd800 && c<=0xdbff) {
      const next=raw.charCodeAt(++i);
      if (!(next>=0xdc00 && next<=0xdfff)) throw Error('invalid lot chat Unicode');
    } else if (c>=0xdc00 && c<=0xdfff) throw Error('invalid lot chat Unicode');
  }
  return raw;
}
export function chatInput(raw: unknown) {
  const v=fields(raw,['sequence','text']),sequence=decimal(v.sequence);
  if(sequence==='0') throw Error('invalid chat sequence');
  return {sequence,text:chatText(v.text)};
}
export function chatAudience<T extends {sender_uid:string}>(messages:T[],source:unknown):T[] {
  if(!Array.isArray(source)||source.length>16384)throw Error('chat ignore metadata unavailable');
  const ignored=new Set(source.map(value=>decimal(value)));
  return messages.filter(value=>!ignored.has(value.sender_uid));
}
export class LotChatStore {
  constructor(private storage: DurableObjectStorage) {
    storage.sql.exec(`CREATE TABLE IF NOT EXISTS lot_chat_messages(id INTEGER PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS lot_chat_clock(id INTEGER PRIMARY KEY CHECK(id=1),sequence INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS lot_chat_senders(avatar_id TEXT PRIMARY KEY,sequence TEXT NOT NULL,receipt TEXT NOT NULL);`);
  }
  read(avatar: string) {
    const prior=this.storage.sql.exec<{sequence:string;receipt:string}>(
      'SELECT sequence,receipt FROM lot_chat_senders WHERE avatar_id=?',avatar).toArray()[0];
    return {viewer_sequence:prior?.sequence??'0',viewer_receipt:prior?JSON.parse(prior.receipt):null,
      messages:this.storage.sql.exec<{value:string}>(
        'SELECT value FROM lot_chat_messages ORDER BY id DESC LIMIT 100').toArray().reverse().map(row=>JSON.parse(row.value))};
  }
  append(avatar:string,name:string,raw:unknown,now:number) {
    const input=chatInput(raw),prior=this.read(avatar);
    if(input.sequence===prior.viewer_sequence) {
      if(prior.viewer_receipt?.text!==input.text) throw Error('chat sequence reused with different text');
      return prior.viewer_receipt;
    }
    if(BigInt(input.sequence)!==BigInt(prior.viewer_sequence)+1n) throw Error('chat sequence is not next');
    const current=this.storage.sql.exec<{sequence:number}>('SELECT sequence FROM lot_chat_clock WHERE id=1').toArray()[0]?.sequence??0;
    if(!Number.isSafeInteger(current)||current>=Number.MAX_SAFE_INTEGER)throw Error('chat identity capacity');
    const id=current+1;
    const receipt={id:String(id),sender_uid:avatar,sender_name:name,sender_sequence:input.sequence,
      channel_id:0,private:false,text:input.text,created_at_ms:now};
    const value=JSON.stringify(receipt);
    this.storage.sql.exec('INSERT INTO lot_chat_messages(id,value) VALUES(?,?)',id,value);
    this.storage.sql.exec('INSERT INTO lot_chat_clock(id,sequence) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET sequence=excluded.sequence',id);
    this.storage.sql.exec('INSERT INTO lot_chat_senders(avatar_id,sequence,receipt) VALUES(?,?,?) ON CONFLICT(avatar_id) DO UPDATE SET sequence=excluded.sequence,receipt=excluded.receipt',avatar,input.sequence,value);
    this.storage.sql.exec('DELETE FROM lot_chat_messages WHERE id<=?',id-100);
    return receipt;
  }
}
