import fs from 'fs';
import path from 'path';
import zlib from 'zlib';
import crypto from 'crypto';

/**
 * Pure Node.js POSIX USTAR tar + GNU ar builder to generate a valid Debian/Ubuntu .deb
 * (`noesis_0.1.0_amd64.deb`) and portable Linux tarball (`Noesis_0.1.0_linux_x86_64.tar.gz`)
 * with exact POSIX permissions (0755 for executables/directories, 0644 for data files).
 */
function createTarHeader(entryPath, size, mode, mtime, typeflag = '0') {
  const buf = Buffer.alloc(512, 0);
  buf.write(entryPath, 0, 100, 'utf8');
  buf.write(mode.toString(8).padStart(7, '0') + '\0', 100, 8, 'ascii');
  buf.write('0000000\0', 108, 8, 'ascii'); // uid 0 (root)
  buf.write('0000000\0', 116, 8, 'ascii'); // gid 0 (root)
  buf.write(size.toString(8).padStart(11, '0') + '\0', 124, 12, 'ascii');
  buf.write(mtime.toString(8).padStart(11, '0') + '\0', 136, 12, 'ascii');
  buf.write('        ', 148, 8, 'ascii'); // checksum placeholder
  buf.write(typeflag, 156, 1, 'ascii');
  buf.write('ustar\0', 257, 6, 'ascii');
  buf.write('00', 263, 2, 'ascii');
  buf.write('root', 265, 32, 'ascii');
  buf.write('root', 297, 32, 'ascii');

  let chksum = 0;
  for (let i = 0; i < 512; i++) chksum += buf[i];
  buf.write(chksum.toString(8).padStart(6, '0') + '\0 ', 148, 8, 'ascii');
  return buf;
}

function buildTarGz(entries) {
  const mtime = Math.floor(Date.now() / 1000);
  const chunks = [];
  for (const e of entries) {
    const isDir = e.type === 'dir';
    const data = isDir ? Buffer.alloc(0) : Buffer.from(e.data);
    const header = createTarHeader(
      e.name,
      data.length,
      e.mode ?? (isDir ? 0o755 : 0o644),
      mtime,
      isDir ? '5' : '0'
    );
    chunks.push(header);
    if (data.length > 0) {
      chunks.push(data);
      const rem = data.length % 512;
      if (rem !== 0) {
        chunks.push(Buffer.alloc(512 - rem, 0));
      }
    }
  }
  chunks.push(Buffer.alloc(1024, 0)); // end-of-archive marker
  return zlib.gzipSync(Buffer.concat(chunks), { level: 9 });
}

function buildArArchive(members) {
  const mtime = Math.floor(Date.now() / 1000);
  const chunks = [Buffer.from('!<arch>\n', 'ascii')];
  for (const m of members) {
    const header = Buffer.alloc(60, 0x20); // space-padded
    header.write(m.name.slice(0, 16), 0, 'ascii');
    header.write(String(mtime).slice(0, 12), 16, 'ascii');
    header.write('0', 28, 'ascii');
    header.write('0', 34, 'ascii');
    header.write('100644', 40, 'ascii');
    header.write(String(m.data.length), 48, 'ascii');
    header.write('`\n', 58, 'ascii');
    chunks.push(header);
    chunks.push(m.data);
    if (m.data.length % 2 !== 0) {
      chunks.push(Buffer.from('\n', 'ascii'));
    }
  }
  return Buffer.concat(chunks);
}

const rootDir = process.cwd();
const pkg = JSON.parse(fs.readFileSync(path.join(rootDir, 'package.json'), 'utf8'));
const version = pkg.version || '0.1.1';

const elfPath = path.join(
  rootDir,
  'src-tauri',
  'target',
  'x86_64-unknown-linux-gnu',
  'release',
  'noesis'
);
const iconPath = path.join(rootDir, 'src-tauri', 'icons', '128x128.png');
const outDir = path.join(rootDir, 'src-tauri', 'target', 'release', 'bundle', 'linux');
fs.mkdirSync(outDir, { recursive: true });

const elfBytes = fs.readFileSync(elfPath);
const iconBytes = fs.readFileSync(iconPath);

// Verify ELF magic (\x7FELF)
if (
  elfBytes[0] !== 0x7f ||
  elfBytes[1] !== 0x45 ||
  elfBytes[2] !== 0x4c ||
  elfBytes[3] !== 0x46
) {
  throw new Error('Target binary is not a valid Linux ELF executable');
}

const desktopEntry = `[Desktop Entry]
Name=Noesis
Comment=NOESIS - Code-Learning Desktop IDE
Exec=/usr/bin/noesis
Icon=noesis
Terminal=false
Type=Application
Categories=Development;IDE;
`;

const md5 = (buf) => crypto.createHash('md5').update(buf).digest('hex');
const desktopBytes = Buffer.from(desktopEntry, 'utf8');
const installedSizeKb = Math.ceil(
  (elfBytes.length + iconBytes.length + desktopBytes.length) / 1024
);

const controlFile = `Package: noesis
Version: ${version}
Section: devel
Priority: optional
Architecture: amd64
Installed-Size: ${installedSizeKb}
Depends: bubblewrap
Maintainer: Noesis Team <team@noesis.ide>
Homepage: https://github.com/kooriishi/Noesis
Description: NOESIS - Code-Learning Desktop IDE
 Local-first code-learning IDE with Tree-sitter AST graph indexing,
 ROS2 & LLM application topology analysis, 4-perspective grounded
 explanations with citation verification, and FSRS-lite spaced review.
`;

const md5sumsFile = [
  `${md5(elfBytes)}  usr/bin/noesis`,
  `${md5(desktopBytes)}  usr/share/applications/noesis.desktop`,
  `${md5(iconBytes)}  usr/share/icons/hicolor/128x128/apps/noesis.png`,
  '',
].join('\n');

const controlTarGz = buildTarGz([
  { name: './', type: 'dir', mode: 0o755 },
  { name: './control', data: Buffer.from(controlFile, 'utf8'), mode: 0o644 },
  { name: './md5sums', data: Buffer.from(md5sumsFile, 'utf8'), mode: 0o644 },
]);

const dataTarGz = buildTarGz([
  { name: './', type: 'dir', mode: 0o755 },
  { name: './usr/', type: 'dir', mode: 0o755 },
  { name: './usr/bin/', type: 'dir', mode: 0o755 },
  { name: './usr/bin/noesis', data: elfBytes, mode: 0o755 },
  { name: './usr/share/', type: 'dir', mode: 0o755 },
  { name: './usr/share/applications/', type: 'dir', mode: 0o755 },
  { name: './usr/share/applications/noesis.desktop', data: desktopBytes, mode: 0o644 },
  { name: './usr/share/icons/', type: 'dir', mode: 0o755 },
  { name: './usr/share/icons/hicolor/', type: 'dir', mode: 0o755 },
  { name: './usr/share/icons/hicolor/128x128/', type: 'dir', mode: 0o755 },
  { name: './usr/share/icons/hicolor/128x128/apps/', type: 'dir', mode: 0o755 },
  { name: './usr/share/icons/hicolor/128x128/apps/noesis.png', data: iconBytes, mode: 0o644 },
]);

const debArchive = buildArArchive([
  { name: 'debian-binary', data: Buffer.from('2.0\n', 'ascii') },
  { name: 'control.tar.gz', data: controlTarGz },
  { name: 'data.tar.gz', data: dataTarGz },
]);

const debPath = path.join(outDir, `noesis_${version}_amd64.deb`);
fs.writeFileSync(debPath, debArchive);

const installSh = `#!/usr/bin/env bash
set -e
PREFIX="\${PREFIX:-/usr/local}"
echo "Installing Noesis v${version} to \$PREFIX..."
install -Dm755 ./noesis "\$PREFIX/bin/noesis"
install -Dm644 ./noesis.desktop /usr/share/applications/noesis.desktop 2>/dev/null || true
install -Dm644 ./noesis.png /usr/share/icons/hicolor/128x128/apps/noesis.png 2>/dev/null || true
echo "Noesis v${version} installed! Run 'noesis' to start."
`;

const tarGzBundle = buildTarGz([
  { name: `noesis-${version}-linux-x86_64/`, type: 'dir', mode: 0o755 },
  { name: `noesis-${version}-linux-x86_64/noesis`, data: elfBytes, mode: 0o755 },
  { name: `noesis-${version}-linux-x86_64/install.sh`, data: Buffer.from(installSh, 'utf8'), mode: 0o755 },
  { name: `noesis-${version}-linux-x86_64/noesis.desktop`, data: desktopBytes, mode: 0o644 },
  { name: `noesis-${version}-linux-x86_64/noesis.png`, data: iconBytes, mode: 0o644 },
]);

const tarGzPath = path.join(outDir, `Noesis_${version}_linux_x86_64.tar.gz`);
fs.writeFileSync(tarGzPath, tarGzBundle);

console.log('Built Linux packages:');
console.log(' -', debPath, `(${(debArchive.length / 1024 / 1024).toFixed(2)} MiB)`);
console.log(' -', tarGzPath, `(${(tarGzBundle.length / 1024 / 1024).toFixed(2)} MiB)`);
