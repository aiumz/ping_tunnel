#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const platform = process.platform;
const arch = process.arch;

// 检测 musl（简化版）
function isMusl() {
  if (platform !== 'linux') return false;
  try {
    const lddOutput = execSync('ldd --version', { encoding: 'utf8' });
    return !lddOutput.includes('GLIBC');
  } catch {
    return false;
  }
}

// 构建库文件名
const platformMap = {
  darwin: 'darwin',
  linux: 'linux',
  win32: 'win32',
};

const archMap = {
  x64: 'x64',
  arm64: 'arm64',
};

const platformName = platformMap[platform];
const archName = archMap[arch];

if (!platformName || !archName) {
  console.warn(`Unsupported platform: ${platform}-${arch}`);
  process.exit(0);
}

const isMuslPlatform = isMusl();
const libName = `ping_tunnel.${platformName}-${archName}${isMuslPlatform ? '-musl' : ''}.node`;
const libPath = path.join(__dirname, '..', libName);

// 检查是否已存在
if (fs.existsSync(libPath)) {
  console.log(`Native binding found: ${libName}`);
  process.exit(0);
}

// 不存在，尝试构建
console.log(`Native binding not found: ${libName}`);
console.log('Attempting to build...');

try {
  // 检查是否有 Rust 工具链
  execSync('rustc --version', { stdio: 'ignore' });
} catch {
  console.error('Rust toolchain not found. Please install Rust: https://rustup.rs/');
  console.error('Or install a pre-built package that includes binaries for your platform.');
  process.exit(1);
}

try {
  execSync('npm run build', {
    stdio: 'inherit',
    cwd: path.join(__dirname, '..')
  });
  console.log('Build completed successfully!');
} catch (error) {
  console.error('Build failed. Please run `npm run build` manually.');
  console.error('Make sure you have Rust toolchain installed: https://rustup.rs/');
  process.exit(1);
}

