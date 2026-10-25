// An HTTP server for the fetch tests, on 127.0.0.1 in Node: it answers one request with what it
// was sent (its method, path, headers and body) as JSON, then stops listening.

const http = process.getBuiltinModule('node:http');

// Listening on a free port, for one request; resolves to its URL.
export function echo() {
  const server = http.createServer((request, response) => {
    const chunks = [];
    request.on('data', (chunk) => chunks.push(chunk));
    request.on('end', () => {
      const body = Buffer.concat(chunks).toString('utf8');
      response.writeHead(200, { 'content-type': 'application/json', connection: 'close' });
      response.end(JSON.stringify({ method: request.method, url: request.url, headers: request.headers, body }));
      server.close();
    });
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(`http://127.0.0.1:${server.address().port}`));
  });
}
