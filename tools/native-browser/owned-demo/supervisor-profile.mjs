import {createContainerEdgeTransport} from './edge-transport.mjs';

// Trusted operator configuration only. Browser requests cannot select a profile,
// origin, binding transport, recovery mode or listening interface.
export function supervisorProfile(hosted) {
  if (hosted === undefined) return {
    hosted: false,
    listenHost: '127.0.0.1',
    accountHeaders(headers) {
      return cleanHeaders({...headers, 'cf-connecting-ip': '127.0.0.1'});
    },
  };
  if (!hosted || Object.keys(hosted).some(key =>
    !['publicOrigin', 'fetchOutbound', 'listenHost'].includes(key)))
    throw Error('Invalid hosted supervisor profile');
  const listenHost = hosted.listenHost ?? '0.0.0.0';
  if (!['0.0.0.0', '127.0.0.1'].includes(listenHost))
    throw Error('Unsupported Container listening interface');
  const transport = createContainerEdgeTransport(hosted);
  return {
    hosted: true, listenHost, publicOrigin: hosted.publicOrigin, transport,
    accountHeaders(headers) {
      // The Worker supplies the actual ingress IP. Do not collapse hosted login
      // throttling onto a fabricated loopback address or accept a missing IP.
      if (typeof headers['cf-connecting-ip'] !== 'string' || !headers['cf-connecting-ip'])
        throw Error('Hosted ingress IP is required');
      return cleanHeaders(headers);
    },
  };
}

function cleanHeaders(headers) {
  const result = {...headers};
  delete result.host;
  delete result['content-length'];
  delete result['x-owned-private'];
  return result;
}
