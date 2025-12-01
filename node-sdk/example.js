const { EdgeClient } = require('./index.js');

const client = new EdgeClient('127.0.0.1:4433', 'my-secret-token-1', '127.0.0.1:11100');
client.connect();


setInterval(() => {
  client.getInboundAddr().then(console.log);
}, 1000);
