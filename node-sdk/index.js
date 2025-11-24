
const nativeBinding = require("./edge.node");
module.exports = nativeBinding
module.exports.EdgeClient = nativeBinding.EdgeClient
module.exports.connectToServer = nativeBinding.connectToServer
nativeBinding.connectToServer("127.0.0.1:4433", "my-secret-token", "127.0.0.1:11100");
setInterval(() => { }, 1000);