import fs from 'fs';
import path from 'path';
import { execSync } from 'child_process';

function getGitHubToken() {
  const out = execSync('git credential fill', {
    input: 'protocol=https\nhost=github.com\n\n',
    encoding: 'utf8',
  });
  const line = out.split(/\r?\n/).find((l) => l.startsWith('password='));
  if (!line) throw new Error('Failed to retrieve GitHub token from git credential');
  return line.slice('password='.length).trim();
}

async function main() {
  const token = getGitHubToken();
  const owner = 'kooriishi';
  const repo = 'Noesis';
  const tagName = 'v0.1.0';
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/vnd.github+json',
    'User-Agent': 'Noesis-Release-Publisher',
  };

  const releaseBody = `## NOESIS v0.1.0 — 代码学习型桌面 IDE (Windows & Linux 正式版)

### 核心特性 (Stages A – E)
- **多语言 AST 与领域拓扑索引**：基于 Tree-sitter 的 Rust / Python / TypeScript / C++ 增量代码图谱解析，内置 **ROS 2**（Node / Topic / Service / Action / Launch / URDF）与 **LLM 应用**（Prompt / Tool / Agent / RAG）领域识别器。
- **4 阶段启发式阅读路线**：自动按「阶段 1 入口层 → 阶段 2 核心主干 → 阶段 3 领域通信 → 阶段 4 底层工具」生成代码导读路径并持久化阅读进度。
- **P0–P7 上下文组装与证据核验**：预算感知的上下文打包器（\`ContextAssembler\`）+ 逐条行号/源码引用自动校验器（\`CitationVerifier\`）+ \`basis_hash\` 本地缓存。
- **分级降维重讲与 FSRS-lite 间隔复习**：支持 4 维度（整体/逻辑/语法/原理）Level 1→3 阶梯降维重讲，以及基于程序事实自检的填空/问答练习与 SM-2/FSRS-lite 复习调度。
- **P0 外发安全控制**：系统级密钥环/700 权限凭证隔离、敏感文件拦截与外发脱敏预览。

---

### 安装包说明
| 平台 | 文件名 | 适用系统与安装方式 |
| :--- | :--- | :--- |
| **Windows (x64)** | \`Noesis_0.1.0_x64-setup.exe\` | Windows 10/11 64 位 NSIS 安装向导（推荐） |
| **Windows (x64)** | \`Noesis_0.1.0_x64_en-US.msi\` | Windows 10/11 64 位 MSI 安装包 |
| **Linux (Debian/Ubuntu amd64)** | \`noesis_0.1.0_amd64.deb\` | Ubuntu / Debian 64 位安装包：\`sudo dpkg -i noesis_0.1.0_amd64.deb\` |
| **Linux (通用 x86_64)** | \`Noesis_0.1.0_linux_x86_64.tar.gz\` | 通用 Linux x86_64 压缩包（含 \`noesis\` ELF 可执行文件与 \`install.sh\`） |
`;

  // 1. Check if release v0.1.0 already exists
  let release;
  const checkRes = await fetch(
    `https://api.github.com/repos/${owner}/${repo}/releases/tags/${tagName}`,
    { headers }
  );

  if (checkRes.ok) {
    release = await checkRes.json();
    console.log(`Found existing release ${tagName} (id=${release.id}), updating notes...`);
    const patchRes = await fetch(
      `https://api.github.com/repos/${owner}/${repo}/releases/${release.id}`,
      {
        method: 'PATCH',
        headers: { ...headers, 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: 'NOESIS v0.1.0 (Windows & Linux)',
          body: releaseBody,
          draft: false,
          prerelease: false,
        }),
      }
    );
    release = await patchRes.json();
  } else {
    console.log(`Creating new GitHub release ${tagName}...`);
    const createRes = await fetch(
      `https://api.github.com/repos/${owner}/${repo}/releases`,
      {
        method: 'POST',
        headers: { ...headers, 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tag_name: tagName,
          target_commitish: 'main',
          name: 'NOESIS v0.1.0 (Windows & Linux)',
          body: releaseBody,
          draft: false,
          prerelease: false,
        }),
      }
    );
    if (!createRes.ok) {
      throw new Error(`Failed to create release: ${createRes.status} ${await createRes.text()}`);
    }
    release = await createRes.json();
  }

  const rootDir = process.cwd();
  const assetsToUpload = [
    {
      name: 'Noesis_0.1.0_x64-setup.exe',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/nsis/Noesis_0.1.0_x64-setup.exe'
      ),
      contentType: 'application/vnd.microsoft.portable-executable',
    },
    {
      name: 'Noesis_0.1.0_x64_en-US.msi',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/msi/Noesis_0.1.0_x64_en-US.msi'
      ),
      contentType: 'application/x-msi',
    },
    {
      name: 'noesis_0.1.0_amd64.deb',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/linux/noesis_0.1.0_amd64.deb'
      ),
      contentType: 'application/vnd.debian.binary-package',
    },
    {
      name: 'Noesis_0.1.0_linux_x86_64.tar.gz',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/linux/Noesis_0.1.0_linux_x86_64.tar.gz'
      ),
      contentType: 'application/gzip',
    },
  ];

  const existingAssets = release.assets || [];

  for (const asset of assetsToUpload) {
    if (!fs.existsSync(asset.filePath)) {
      throw new Error(`Missing release asset file: ${asset.filePath}`);
    }

    const prev = existingAssets.find((a) => a.name === asset.name);
    if (prev) {
      console.log(`Deleting previous asset ${asset.name} (id=${prev.id})...`);
      await fetch(
        `https://api.github.com/repos/${owner}/${repo}/releases/assets/${prev.id}`,
        { method: 'DELETE', headers }
      );
    }

    const fileBytes = fs.readFileSync(asset.filePath);
    console.log(
      `Uploading ${asset.name} (${(fileBytes.length / 1024 / 1024).toFixed(2)} MiB)...`
    );
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
    if (!upRes.ok) {
      throw new Error(`Failed to upload ${asset.name}: ${upRes.status} ${await upRes.text()}`);
    }
    const uploaded = await upRes.json();
    console.log(` -> Uploaded: ${uploaded.browser_download_url}`);
  }

  console.log(`\nRelease published successfully: ${release.html_url}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
