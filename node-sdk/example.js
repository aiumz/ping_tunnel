const { EdgeClient } = require('./index.js');

const client = new EdgeClient('192.168.2.6:4433', '192', '127.0.0.1:11100');
client.connect();


setInterval(() => { }, 1000);
