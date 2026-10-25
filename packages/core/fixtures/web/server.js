// A websocket server for the feed's tests, on 127.0.0.1 in Node: node:http's `upgrade`, the
// RFC 6455 handshake, and text frames both ways (client frames unmasked). It answers a close
// and a protocol ping, and nothing else on its own: the test accepts, reads, sends and closes.
// No extensions, no binary, no fragments sent.

const http = process.getBuiltinModule('node:http');
const crypto = process.getBuiltinModule('node:crypto');

// RFC 6455 section 1.3: appended to the client's key before hashing.
const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

// What arrived and was not yet asked for, and who is waiting for the next one.
class Queue {
  constructor() {
    this.items = [];
    this.waiting = [];
    this.ended = null;
  }

  push(item) {
    const waiter = this.waiting.shift();
    if (waiter) waiter.resolve(item);
    else this.items.push(item);
  }

  end(why) {
    this.ended = why;
    for (const waiter of this.waiting.splice(0)) waiter.reject(new Error(why));
  }

  // The next item, or a rejection after `ms` or once nothing more can come.
  next(ms, what) {
    if (this.items.length > 0) return Promise.resolve(this.items.shift());
    if (this.ended) return Promise.reject(new Error(this.ended));
    return new Promise((resolve, reject) => {
      const waiter = {};
      const timer = setTimeout(() => {
        this.waiting = this.waiting.filter((each) => each !== waiter);
        reject(new Error(`no ${what} within ${ms} ms`));
      }, ms);
      waiter.resolve = (item) => {
        clearTimeout(timer);
        resolve(item);
      };
      waiter.reject = (error) => {
        clearTimeout(timer);
        reject(error);
      };
      this.waiting.push(waiter);
    });
  }
}

export class Server {
  constructor(server) {
    this.server = server;
    this.peers = new Queue();
    this.sockets = new Set();
    server.on('upgrade', (request, socket, head) => {
      const key = request.headers['sec-websocket-key'];
      const accept = crypto.createHash('sha1').update(key + GUID).digest('base64');
      socket.write(
        'HTTP/1.1 101 Switching Protocols\r\n' +
          'Upgrade: websocket\r\n' +
          'Connection: Upgrade\r\n' +
          `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
      );
      this.sockets.add(socket);
      socket.on('close', () => this.sockets.delete(socket));
      this.peers.push(new Peer(socket, head));
    });
  }

  // Listening on a free port.
  static listen() {
    const server = http.createServer((_, response) => response.writeHead(426).end());
    return new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(0, '127.0.0.1', () => resolve(new Server(server)));
    });
  }

  url() {
    return `ws://127.0.0.1:${this.server.address().port}`;
  }

  // The next connection to finish its handshake.
  accept(ms) {
    return this.peers.next(ms, 'connection');
  }

  // Stops listening and cuts every connection.
  close() {
    this.server.close();
    for (const socket of this.sockets) socket.destroy();
    this.peers.end('the server closed');
  }
}

export class Peer {
  constructor(socket, head) {
    this.socket = socket;
    this.buffer = Buffer.alloc(0);
    this.fragments = [];
    this.texts = new Queue();
    this.ends = new Queue();
    this.closing = false;
    socket.on('data', (chunk) => this.read(chunk));
    socket.on('error', () => {});
    socket.on('close', () => {
      this.texts.end('the socket closed');
      this.ends.push(true);
    });
    if (head.length > 0) this.read(head);
  }

  // The next text frame the client sent.
  text(ms) {
    return this.texts.next(ms, 'text');
  }

  // Resolves once the connection is gone.
  ended(ms) {
    return this.ends.next(ms, 'close');
  }

  send(text) {
    this.write(0x1, Buffer.from(text, 'utf8'));
  }

  // A close frame with 1000, then the end of the stream.
  close() {
    this.closing = true;
    this.write(0x8, Buffer.from([0x03, 0xe8]));
    this.socket.end();
  }

  read(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const buffer = this.buffer;
      if (buffer.length < 2) return;
      const fin = (buffer[0] & 0x80) !== 0;
      const opcode = buffer[0] & 0x0f;
      const masked = (buffer[1] & 0x80) !== 0;
      let length = buffer[1] & 0x7f;
      let at = 2;
      if (length === 126) {
        if (buffer.length < 4) return;
        length = buffer.readUInt16BE(2);
        at = 4;
      } else if (length === 127) {
        if (buffer.length < 10) return;
        length = Number(buffer.readBigUInt64BE(2));
        at = 10;
      }
      const key = masked ? at : -1;
      if (masked) at += 4;
      if (buffer.length < at + length) return;
      const payload = Buffer.from(buffer.subarray(at, at + length));
      if (masked) {
        for (let index = 0; index < payload.length; index += 1) payload[index] ^= buffer[key + (index & 3)];
      }
      this.buffer = buffer.subarray(at + length);
      this.frame(fin, opcode, payload);
    }
  }

  frame(fin, opcode, payload) {
    if (opcode === 0x1 || opcode === 0x0) {
      this.fragments.push(payload);
      if (fin) this.texts.push(Buffer.concat(this.fragments.splice(0)).toString('utf8'));
    } else if (opcode === 0x8) {
      if (!this.closing) {
        this.closing = true;
        this.write(0x8, payload.subarray(0, 2));
      }
      this.socket.end();
    } else if (opcode === 0x9) {
      this.write(0xa, payload);
    }
  }

  // One unmasked frame, as a server sends.
  write(opcode, payload) {
    if (this.socket.destroyed || this.socket.writableEnded) return;
    let head;
    if (payload.length < 126) {
      head = Buffer.from([0x80 | opcode, payload.length]);
    } else if (payload.length < 0x10000) {
      head = Buffer.alloc(4);
      head[0] = 0x80 | opcode;
      head[1] = 126;
      head.writeUInt16BE(payload.length, 2);
    } else {
      head = Buffer.alloc(10);
      head[0] = 0x80 | opcode;
      head[1] = 127;
      head.writeBigUInt64BE(BigInt(payload.length), 2);
    }
    this.socket.write(Buffer.concat([head, payload]));
  }
}

// A timer the test can race and cancel, so none outlives its test.
export class Deadline {
  constructor(ms) {
    this.promise = new Promise((resolve) => {
      this.timer = setTimeout(resolve, ms);
    });
  }

  cancel() {
    clearTimeout(this.timer);
  }
}
