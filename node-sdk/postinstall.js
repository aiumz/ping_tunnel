#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const platform = process.platform;
const arch = process.arch;

// 构建库文件名
const platformMap = {
  darwin: 'darwin',
  linux: 'linux',
  win32: 'win',
};

const archMap = {
  x64: 'x64',
  arm64: 'arm',
};

const packageVersion = JSON.parse(fs.readFileSync(path.join(__dirname, 'package.json'), 'utf8')).version;
const platformName = platformMap[platform];
const archName = archMap[arch];

if (!platformName || !archName) {
  console.warn(`Unsupported platform: ${platform}-${arch}`);
  process.exit(0);
}

const libPath = path.join(__dirname, "edge.node");

if (fs.existsSync(libPath)) {
  console.log(`Native binding found: ${libPath}`);
  process.exit(0);
} else {
  fetch(`https://github.com/aiumz/ping_tunnel/releases/download/v${packageVersion}/edge-${platformName}-${archName}.node`)
    .then(response => response.arrayBuffer())
    .then(buffer => {
      fs.writeFileSync(libPath, Buffer.from(buffer));
      console.log(`Native binding downloaded: ${libPath}`);
      process.exit(0);
    });
}
