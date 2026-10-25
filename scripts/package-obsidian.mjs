// Shared by Tauri's build hook and all desktop release scripts. No vault data is read.
import { readFileSync, mkdirSync, writeFileSync, renameSync, rmSync } from "node:fs";
import { dirname, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const plugin = join(root, "integrations/obsidian-arsenal");
const args = process.argv.slice(2);
let output = join(root, "release-assets");
let install = true, tests = true;
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--output-dir" && args[i + 1]) output = resolve(args[++i]);
  else if (args[i] === "--skip-install") install = false;
  else if (args[i] === "--skip-tests") tests = false;
  else throw new Error(`Unknown or incomplete option: ${args[i]}`);
}
function run(commandArgs) {
  // Only fixed, internal pnpm arguments enter cmd.exe; paths stay in cwd/fs APIs.
  const windows = process.platform === "win32";
  const result = spawnSync(windows ? (process.env.ComSpec || "cmd.exe") : "pnpm",
    windows ? ["/d", "/s", "/c", `pnpm ${commandArgs.join(" ")}`] : commandArgs,
    { cwd: plugin, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Plugin command failed: pnpm ${commandArgs.join(" ")}`);
}
if (install) run(["install", "--frozen-lockfile"]);
run(["build"]);
if (tests) run(["test"]);

const manifest = JSON.parse(readFileSync(join(plugin, "manifest.json"), "utf8"));
const pluginPackage = JSON.parse(readFileSync(join(plugin, "package.json"), "utf8"));
const app = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
if (manifest.id !== "arsenal-link" || manifest.version !== pluginPackage.version) {
  throw new Error("Plugin manifest identity/version does not match package.json");
}
if (!/^\d+\.\d+\.\d+(?:[-+][\w.-]+)?$/.test(app.version)) throw new Error("Invalid application version");

// A small deterministic ZIP (stored entries). Fixed timestamps make the shared
// cross-platform asset identical when release jobs merge their artifacts.
function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}
const chunks = [], directory = [];
let offset = 0;
for (const file of ["main.js", "manifest.json", "README.md"]) {
  const data = Buffer.from(readFileSync(join(plugin, file), "utf8").replace(/\r\n/g, "\n"));
  if (!data.length) throw new Error(`Empty plugin file: ${file}`);
  const name = Buffer.from(`${manifest.id}/${file}`);
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20, 4);
  local.writeUInt16LE(0x800, 6); local.writeUInt16LE(33, 12); // UTF-8; 1980-01-01
  local.writeUInt32LE(crc32(data), 14);
  local.writeUInt32LE(data.length, 18); local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(name.length, 26);
  const entry = Buffer.alloc(46);
  entry.writeUInt32LE(0x02014b50); entry.writeUInt16LE(20, 4); entry.writeUInt16LE(20, 6);
  local.copy(entry, 8, 6, 28);
  entry.writeUInt32LE(offset, 42);
  chunks.push(local, name, data); directory.push(entry, name);
  offset += local.length + name.length + data.length;
}
const central = Buffer.concat(directory);
const end = Buffer.alloc(22);
end.writeUInt32LE(0x06054b50); end.writeUInt16LE(3, 8); end.writeUInt16LE(3, 10);
end.writeUInt32LE(central.length, 12); end.writeUInt32LE(offset, 16);
mkdirSync(output, { recursive: true });
const dest = join(output, `arsenal_${app.version}_obsidian-plugin.zip`);
const temp = `${dest}.${process.pid}.tmp`;
try {
  writeFileSync(temp, Buffer.concat([...chunks, central, end]));
  renameSync(temp, dest);
} finally { rmSync(temp, { force: true }); }
console.log(`Obsidian plugin: ${dest}`);
