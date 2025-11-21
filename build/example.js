const { connectToServer } = require('./index.js');

console.log('Starting edge client...');
connectToServer('74.48.58.156:4433', 'my-secret-token1', '127.0.0.1:11100');
console.log('Edge client started in background');

setInterval(() => {
  console.log('Edge client is running...');
}, 10000);

