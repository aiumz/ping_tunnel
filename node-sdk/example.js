const { EdgeClient } = require('./index.js');

const client = new EdgeClient('192.168.1.24:4433', 'aiumz', '127.0.0.1:11100');
client.connect();


setInterval(() => { }, 1000);
