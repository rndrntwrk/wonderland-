// Trusted native-supervisor transport for the Container's private outbound
// binding. Does not open a server, expose RPC publicly, issue grants or retry.
const privateRpc = 'http://owned-private.invalid/rpc';
const accountPaths = new Set([
  '/native-account/login', '/native-account/roster',
  '/native-account/logout', '/native-account/rotate',
  '/native-account/create',
  '/native-account/creation-options',
]);
export function createContainerEdgeTransport({publicOrigin, fetchOutbound=fetch}) {
  const origin = new URL(publicOrigin);
  if (origin.protocol !== 'https:' || publicOrigin !== origin.origin)
    throw Error('Hosted public origin must be an exact HTTPS origin');
  if (typeof fetchOutbound !== 'function') throw Error('Missing private outbound transport');
  let closed=false;
  return {
    async dispatchFetch(input, init) {
      if (closed) throw Error('Native control transport is closed');
      const request=new Request(input,init),url=new URL(request.url);
      if (request.method!=='POST' || request.headers.get('content-type')!=='application/json')
        throw Error('Unsupported native control request');
      if (request.url===privateRpc) {
        if (request.headers.has('origin')) throw Error('Browser origin is not supervisor authority');
        return fetchOutbound(request);
      }
      if (url.origin!==origin.origin || url.search || url.hash || !accountPaths.has(url.pathname)
          || request.headers.get('origin')!==origin.origin)
        throw Error('Unsupported native account forwarding');
      const forwarded=new Request('http://owned-private.invalid/account-http'+url.pathname,request);
      forwarded.headers.delete('x-owned-private');
      forwarded.headers.delete('host');
      forwarded.headers.delete('content-length');
      return fetchOutbound(forwarded);
    },
    async dispose() {closed=true;},
  };
}
