const { EdgeClient } = require('./index.js');

const client = new EdgeClient('221.11.37.114:41493', '221', '127.0.0.1:8000');
client.connect();


setInterval(() => { }, 1000);
