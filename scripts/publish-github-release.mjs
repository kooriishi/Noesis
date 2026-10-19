import fs from 'fs';
import path from 'path';
import { execSync } from 'node:child_process';

/**
 * 发布指定版本的 GitHub Release 并上传安装包。
 *
 * 用法：node scripts/publish-github-release.mjs [version]
 *   例：node scripts/publish-github-release.mjs 0.1.3
 *
 * 版本号默认取 package.json 的 version。发布说明取自 docs/releases/v<version>.md。
 * 认证：优先使用 `gh auth token`，失败时回退 `git credential fill`。
 */

function getGitHubToken() {
  // 1. 优先使用 GitHub CLI 的登录态
  try {
    const token = execSync('gh auth token', { encoding: 'utf8' }).trim();
    if (token) return token;
  } catch {
    // 继续回退
  }

  // 2. 回退到 git credential
  const out = execSync('git credential fill', {
    input: 'protocol=https\nhost=github.com\n\n',
    encoding: 'utf8',
  });
  const line = out.split(/\r?\n/).find((l) => l.startsWith('password='));
  if (!line) throw new Error('Failed to retrieve GitHub token (tried gh auth token and git credential)');
  return line.slice('password='.length).trim();
}

const owner = 'kooriishi';
const repo = 'Noesis';

const rootDir = process.cwd();
const pkg = JSON.parse(fs.readFileSync(path.join(rootDir, 'package.json'), 'utf8'));
const version = (process.argv[2] || pkg.version || '').replace(/^v/, '');
if (!version) throw new Error('无法确定版本号：请在 package.json 中设置 version，或作为参数传入');

const tagName = `v${version}`;
const releaseName = `NOESIS v${version} (Windows & Linux)`;

const notesPath = path.join(rootDir, 'docs', 'releases', `${tagName}.md`);
if (!fs.existsSync(notesPath)) {
  throw new Error(`缺少发布说明文件: ${notesPath}`);
}
const releaseBody = fs.readFileSync(notesPath, 'utf8');

const assetsToUpload = [
  {
    name: `Noesis_${version}_x64-setup.exe`,
    filePath: path.join(rootDir, 'src-tauri/target/release/bundle/nsis', `Noesis_${version}_x64-setup.exe`),
    contentType: 'application/vnd.microsoft.portable-executable',
  },
  {
    name: `Noesis_${version}_x64_en-US.msi`,
    filePath: path.join(rootDir, 'src-tauri/target/release/bundle/msi', `Noesis_${version}_x64_en-US.msi`),
    contentType: 'application/x-msi',
  },
  {
    name: `noesis_${version}_amd64.deb`,
    filePath: path.join(rootDir, 'src-tauri/target/release/bundle/linux', `noesis_${version}_amd64.deb`),
    contentType: 'application/vnd.debian.binary-package',
  },
  {
    name: `Noesis_${version}_linux_x86_64.tar.gz`,
    filePath: path.join(rootDir, 'src-tauri/target/release/bundle/linux', `Noesis_${version}_linux_x86_64.tar.gz`),
    contentType: 'application/gzip',
  },
];

async function main() {
  // 上传前先确认所有产物存在，避免发到一半失败
  for (const asset of assetsToUpload) {
    if (!fs.existsSync(asset.filePath)) {
      throw new Error(`缺少发布产物: ${asset.filePath}\n请先执行 pnpm tauri build 与 node scripts/build-linux-packages.mjs`);
    }
  }

  const token = getGitHubToken();
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/vnd.github+json',
    'User-Agent': 'Noesis-Release-Publisher',
  };

  let release;
  const checkRes = await fetch(`https://api.github.com/repos/${owner}/${repo}/releases/tags/${tagName}`, { headers });

  if (checkRes.ok) {
    release = await checkRes.json();
    console.log(`已存在 Release ${tagName} (id=${release.id})，更新说明与产物...`);
    const patchRes = await fetch(`https://api.github.com/repos/${owner}/${repo}/releases/${release.id}`, {
      method: 'PATCH',
      headers: { ...headers, 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name: releaseName,
        body: releaseBody,
        draft: false,
        prerelease: false,
      }),
    });
    if (!patchRes.ok) throw new Error(`更新 Release 失败: ${patchRes.status} ${await patchRes.text()}`);
    release = await patchRes.json();
  } else {
    console.log(`创建新的 Release ${tagName}...`);
    const createRes = await fetch(`https://api.github.com/repos/${owner}/${repo}/releases`, {
      method: 'POST',
      headers: { ...headers, 'Content-Type': 'application/json' },
      body: JSON.stringify({
        tag_name: tagName,
        target_commitish: 'main',
        name: releaseName,
        body: releaseBody,
        draft: false,
        prerelease: false,
      }),
    });
    if (!createRes.ok) throw new Error(`创建 Release 失败: ${createRes.status} ${await createRes.text()}`);
    release = await createRes.json();
  }

  const existingAssets = release.assets || [];

  for (const asset of assetsToUpload) {
    const prev = existingAssets.find((a) => a.name === asset.name);
    if (prev) {
      console.log(`删除同名旧产物 ${asset.name} (id=${prev.id})...`);
      await fetch(`https://api.github.com/repos/${owner}/${repo}/releases/assets/${prev.id}`, {
        method: 'DELETE',
        headers,
      });
    }

    const fileBytes = fs.readFileSync(asset.filePath);
    console.log(`上传 ${asset.name} (${(fileBytes.length / 1024 / 1024).toFixed(2)} MiB)...`);
    const uploadUrl = `https://uploads.github.com/repos/${owner}/${repo}/releases/${release.id}/assets?name=${encodeURIComponent(asset.name)}`;
    const upRes = await fetch(uploadUrl, {
      method: 'POST',
      headers: {
        ...headers,
        'Content-Type': asset.contentType,
        'Content-Length': String(fileBytes.length),
      },
      body: fileBytes,
    });
    if (!upRes.ok) throw new Error(`上传 ${asset.name} 失败: ${upRes.status} ${await upRes.text()}`);
    const uploaded = await upRes.json();
    console.log(` -> 已上传: ${uploaded.browser_download_url}`);
  }

  console.log(`\nRelease 发布成功: ${release.html_url}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
