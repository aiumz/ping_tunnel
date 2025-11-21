const { EdgeClient } = require('./index.js');

// 启动 edge 客户端
console.log('Starting edge client...');
const client = new EdgeClient('127.0.0.1:4433', 'my-secret-token1', '127.0.0.1:11100');
client.connect();
console.log('Edge client started in background');



setInterval(() => { }, 10000);