/**
 * 跨平台构建 napi 模块
 * 输出目录: ./dist/<platform>
 * Node ABI/N-API 使用本地 Node.js 版本
 */

const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const targets = [
  { name: "mac-x64", rust: "x86_64-apple-darwin" },
  { name: "mac-arm", rust: "aarch64-apple-darwin" },
  { name: "linux-x64", rust: "x86_64-unknown-linux-gnu" },
  { name: "linux-arm", rust: "aarch64-unknown-linux-gnu" },
  { name: "win-x64", rust: "x86_64-pc-windows-gnu" },
];

// 读取本地 Node.js 的 N-API 版本
const napiVersion = parseInt(execSync("node -p process.versions.napi").toString());

const moduleName = "my_module";

targets.forEach(target => {
  const outDir = path.resolve("dist", target.name);
  fs.mkdirSync(outDir, { recursive: true });

  console.log(`\n=== Building ${moduleName} for ${target.name} ===`);
  const cmd = [
    "./node_modules/.bin/napi",
    "build",
    "--release",
    `--target ${target.rust}`,
    `--napi-version ${napiVersion}`,
    `-o ${outDir}`,
    "--verbose"
  ].join(" ");

  try {
    execSync(cmd, { stdio: "inherit" });
    console.log(`✔ Built ${moduleName} for ${target.name}`);
  } catch (err) {
    console.error(`✖ Failed to build ${moduleName} for ${target.name}`);
    process.exit(1);
  }
});
