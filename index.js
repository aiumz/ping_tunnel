const { existsSync, readFileSync } = require('fs');
const { join } = require('path');

const platform = process.platform;
const arch = process.arch;

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
  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

let nativeBinding = null;
let loadError = null;

function isMusl() {
  let musl = false;
  if (process.platform === 'linux') {
    musl = isMuslFromFilesystem();
    if (!musl) {
      musl = isMuslFromReport();
    }
    if (!musl) {
      musl = isMuslFromLdd();
    }
  }
  return musl;
}

function isMuslFromFilesystem() {
  try {
    return readFileSync('/usr/bin/ldd', 'utf8').includes('musl');
  } catch {
    return false;
  }
}

function isMuslFromReport() {
  const report = process.report.getReport();
  if (typeof report === 'object') {
    const glibcVersionRuntime = report.header?.glibcVersionRuntime;
    return !glibcVersionRuntime;
  }
  return false;
}

function isMuslFromLdd() {
  try {
    const { execSync } = require('child_process');
    const lddOutput = execSync('ldd --version', { encoding: 'utf8' });
    return !lddOutput.includes('GLIBC');
  } catch {
    return false;
  }
}

function loadBinding() {
  if (nativeBinding) {
    return nativeBinding;
  }

  if (loadError) {
    throw loadError;
  }

  const isLinux = platform === 'linux';
  const isMuslPlatform = isLinux && isMusl();
  const libName = `ping_tunnel.${platformName}-${archName}${isMuslPlatform ? '-musl' : ''}.node`;
  const localFile = join(__dirname, libName);

  if (!existsSync(localFile)) {
    loadError = new Error(`Cannot find module ${libName}, please run \`npm run build\` to build it.`);
    throw loadError;
  }

  try {
    nativeBinding = require(localFile);
  } catch (error) {
    loadError = error;
    throw error;
  }

  return nativeBinding;
}

module.exports = loadBinding();

