const path = require("path");
const edgePath = process.env.EDGE_PATH || path.join(__dirname, "edge.node");
const nativeBinding = require(edgePath);
module.exports = nativeBinding
module.exports.EdgeClient = nativeBinding.EdgeClient
module.exports.connectToServer = nativeBinding.connectToServer
