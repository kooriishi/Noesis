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
  const tagName = 'v0.1.1';
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/vnd.github+json',
    'User-Agent': 'Noesis-Release-Publisher',
  };

  const releaseBody = `## NOESIS v0.1.1 — 深度导读质量升级与工程问答增强版 (Windows & Linux)

本版本依据第三轮评估优化方案，全面落地 **F1 至 F7** 核心升级，重点攻克了解读泛化、提示词不可调、问答上下文缺失与卡点复习断层等问题，带来更深入、可验证的代码学习体验。

---

### 🌟 核心特性与优化 (F1 – F7)

- **F1 提示词专家准则与降维策略重构**：
  - 重构《NOESIS 代码深度导读专家》系统准则，注入 P0 证据锁定、反事实设计动机与 4 视角针对性降维机制；
  - 内置通用泛套话黑名单与自动清洗机制，杜绝空洞套话。
- **F2 四层导读质量门禁与「深入」模式**：
  - 导读四层（整体、逻辑、语法、原理）详情默认展开；
  - 新增「标准」与「深入」模式切换，深入模式将上下文预算扩增至 24k tokens，LLM 最大输出提高至 8192 tokens；
  - 引入导读质量门禁（\`QualityGate\`），自动打回识别过浅或泛化的回答并针对性补充行号追踪。
- **F3 历史解读完整持久化管理**：
  - 每次解读自动沉淀至本地 SQLite 数据库；
  - 导读面板内置历史选择器，支持关键词搜索、置顶收藏、快捷重命名与软删除。
- **F4 工程针对性问答与源码上下文 Chip**：
  - 在编辑器中选中任意代码片段一键发起问答，携带文件范围与上下文芯片（Context Chip）；
  - 全流程通过 Rust 预算上下文装配器与 P0 外发脱敏审查，支持流式 Token 生成与源码引用行号自动核验。
- **F5 智能体提示词预设与安全分层体系**：
  - 在设置中新增「智能体提示词」专区，实现 L0 锁定硬规则与 L1 自定义教学人设安全解耦；
  - 支持创建多套人设预设并一键切换，提供实时生效提示词预览与 Token 消耗估算。
- **F6 「看不懂」卡点闭环与梯次降维**：
  - 卡点详情界面支持 L1/L2/L3 梯次降维重讲、逐行代码标注高亮展示与行号跳转；
  - 攻克卡点时提供「看懂了」即时正向反馈，并支持全量卡点汇编一键导出为 Markdown。
- **F7 评测基线与全量界面细节润色**：
  - 导读卡片内置单卡有用/太泛/有误反馈按钮，直接沉淀至本地评测基线并在设置中展示统计看板；
  - 全局移除原生 \`alert()\` 弹窗，统一采用现代化 Toast 反馈机制；
  - 全量 UI 色彩与圆角统一收敛至 Catppuccin Mocha CSS 设计变量。

---

### 安装包说明
| 平台 | 文件名 | 适用系统与安装方式 |
| :--- | :--- | :--- |
| **Windows (x64)** | \`Noesis_0.1.1_x64-setup.exe\` | Windows 10/11 64 位 NSIS 安装向导（推荐） |
| **Windows (x64)** | \`Noesis_0.1.1_x64_en-US.msi\` | Windows 10/11 64 位 MSI 安装包 |
| **Linux (Debian/Ubuntu amd64)** | \`noesis_0.1.1_amd64.deb\` | Ubuntu / Debian 64 位安装包：\`sudo dpkg -i noesis_0.1.1_amd64.deb\` |
| **Linux (通用 x86_64)** | \`Noesis_0.1.1_linux_x86_64.tar.gz\` | 通用 Linux x86_64 压缩包（含 \`noesis\` ELF 可执行文件与 \`install.sh\`） |
`;

  // 1. Check if release v0.1.1 already exists
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
          name: 'NOESIS v0.1.1 (Windows & Linux)',
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
          name: 'NOESIS v0.1.1 (Windows & Linux)',
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
      name: 'Noesis_0.1.1_x64-setup.exe',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/nsis/Noesis_0.1.1_x64-setup.exe'
      ),
      contentType: 'application/vnd.microsoft.portable-executable',
    },
    {
      name: 'Noesis_0.1.1_x64_en-US.msi',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/msi/Noesis_0.1.1_x64_en-US.msi'
      ),
      contentType: 'application/x-msi',
    },
    {
      name: 'noesis_0.1.1_amd64.deb',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/linux/noesis_0.1.1_amd64.deb'
      ),
      contentType: 'application/vnd.debian.binary-package',
    },
    {
      name: 'Noesis_0.1.1_linux_x86_64.tar.gz',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/linux/Noesis_0.1.1_linux_x86_64.tar.gz'
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
