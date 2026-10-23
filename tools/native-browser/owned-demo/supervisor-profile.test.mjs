import assert from 'node:assert/strict';
import test from 'node:test';
import {supervisorProfile} from './supervisor-profile.mjs';

test('default profile retains loopback and local account throttling identity', () => {
  const profile = supervisorProfile();
  assert.equal(profile.hosted, false);
  assert.equal(profile.listenHost, '127.0.0.1');
  assert.deepEqual(profile.accountHeaders({host:'local','cf-connecting-ip':'spoof'}),
    {'cf-connecting-ip':'127.0.0.1'});
});

test('hosted profile requires explicit HTTPS origin and restricts interfaces', () => {
  for (const hosted of [null, {}, {publicOrigin:'http://example.test'},
    {publicOrigin:'https://example.test/',fetchOutbound:fetch},
    {publicOrigin:'https://example.test',listenHost:'192.0.2.1'},
    {publicOrigin:'https://example.test',privateRpc:'https://elsewhere.test'}])
    assert.throws(() => supervisorProfile(hosted));
});

test('hosted account forwarding retains actual ingress IP and strips private headers', async () => {
  const seen=[];
  const profile=supervisorProfile({publicOrigin:'https://example.test',listenHost:'127.0.0.1',
    fetchOutbound:async request => {seen.push(request);return Response.json({ok:true});}});
  assert.equal(profile.listenHost,'127.0.0.1');
  assert.throws(() => profile.accountHeaders({origin:'https://example.test'}),/ingress IP/);
  const headers=profile.accountHeaders({origin:'https://example.test','content-type':'application/json',
    'cf-connecting-ip':'2001:db8::1',host:'internal','content-length':'9','x-owned-private':'never forward'});
  await profile.transport.dispatchFetch('https://example.test/native-account/login',
    {method:'POST',headers,body:'{}'});
  assert.equal(seen[0].headers.get('cf-connecting-ip'),'2001:db8::1');
  assert.equal(seen[0].headers.has('x-owned-private'),false);
  await profile.transport.dispose();
  await assert.rejects(profile.transport.dispatchFetch('http://owned-private.invalid/rpc',
    {method:'POST',headers:{'content-type':'application/json'},body:'{}'}),/closed/);
});
