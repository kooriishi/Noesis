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
  const tagName = 'v0.1.2';
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/vnd.github+json',
    'User-Agent': 'Noesis-Release-Publisher',
  };

  const releaseBody = `## NOESIS v0.1.2 — Linux稳定性与架构解耦升级版 (Windows & Linux)

本版本严格按照第四轮评估与优化方案（G1 – G7 步骤）全面落地，彻底消除 Linux 下 SQLite 数据库写锁冲突，重构双端共享命令架构，升级四层解读/看不懂答疑/智能问答三大 AI 板块，引入富文本公式图表渲染与 C/C++ 编译沙箱，并带来平滑的可拖拽底部抽屉。

---

### 🌟 核心特性与优化 (G1 – G7)

- **G1 彻底消除 Linux 下 SQLite 数据库锁与多实例互斥**：
  - 数据库默认开启 WAL（Write-Ahead Logging）模式、\`busy_timeout=10000ms\` 与 \`synchronous=NORMAL\`；
  - 写事务严格采用 \`TransactionBehavior::Immediate\`，彻底杜绝多连接写锁升级死锁；
  - 核心查询表在 \`file_id\` 上建立联合索引，全面加速符号查找；
  - Linux 端基于 \`fs2\` 文件排他锁实现单实例检测与 URL 激活跳转，Windows 端基于 \`tauri-plugin-single-instance\` 实现窗口前置激活；
  - 引入 30 秒心跳自愈与生命周期监控（\`/__noesis_heartbeat__\`），异常崩溃后自动清理锁；
  - 增量索引在后台线程运行，集成取消原语（\`AtomicBool\`），避免长时间阻塞数据库；
  - 提供友好中文报错与重试提示，杜绝底层裸抛 \`SQLITE_BUSY\`。

- **G2 共享命令架构与 Linux 极速微内核 (noesis-commands)**：
  - 抽离出平台无关的共享命令库 \`noesis-commands\`，实现 62 个 API 前端命令 100% 覆盖与严格单元测试；
  - Windows 端 Tauri 与 Linux 独立桌面端采用同一套业务逻辑与参数转发机制；
  - Linux 端新增原生 SSE 流式事件通道（\`/__noesis_events__?channel=<id>\`），支持逐字流式打字与进度分发；
  - Linux 静态前端资源直接嵌入二进制（\`dist/\`），支持优先加载本地构建产物及反向代理 \`NOESIS_DEV_URL\`。

- **G3 & G5 四层解读、看不懂答疑与智能问答全面升级**：
  - 四层解读全面拆分为 4 次独立并发调用（\`explain_layer\`），卡片独立加载、流式出字与单卡重试；
  - 输出格式由 JSON 全面重写为带结构化二级标题的纯 Markdown，严格遵守字数下限并给出 ROS2 C++ 和大模型 Python 的示范样例；
  - 提示词版本升级至 \`PROMPT_VERSION = "3.0"\`，引入代码引用标记 \`[Lxx-Lyy]\`，由程序质检门禁（\`QualityGate\`）严格核验行号与套话；
  - 「看不懂答疑」重构为 Rust 原生处理（\`explain_confusion\`），针对整体/逻辑/语法/原理卡点梯次降维并提供逐行精读注释；
  - 「智能问答」接入 14 种本地工具循环智能体（\`ask_project_question\`），规划思考过程全程可见，且受严格 Token 预算与安全沙箱约束。

- **G4 现代化 Markdown 渲染引擎（公式、Mermaid 与图表）**：
  - 全面替换手写正则解析器，采用 \`react-markdown\` + \`remark-gfm\` + \`remark-math\` + \`rehype-katex\`；
  - 支持 GFM 表格、任务列表、删除线、行内公式与块级 LaTeX 公式渲染；
  - 异步按需加载 \`mermaid\`，以严格安全模式（\`securityLevel: 'strict'\`）渲染数据流向图与时序图，自带深色主题与语法容错；
  - 代码引用标记 \`[Lxx-Lyy]\` 自动高亮为可点击跳转的引用徽章。

- **G6 可拖拽高度的底部抽屉与侧栏滚动体验重构**：
  - 底部草稿沙盒与复盘抽屉顶部配备 4px 拖拽把手，支持鼠标在 120px 至 (窗口高度 - 160px) 范围内平滑拖拽；
  - 抽屉高度记忆持久化（\`localStorage\`），支持双击把手在 120px / 280px / 最大高度之间循环切换；
  - 编辑器容器自适应伸缩，完全避免 CodeMirror 实例重建，光标与选择状态不丢失；
  - 修复 AI 侧栏内部滚动嵌套问题，统一单根容器滚动与横向表格/代码滚动，流式输出支持「触底自动跟随」与「上滑保持原位」。

- **G7 C/C++ 代码编译与运行沙箱 (noesis-runner)**：
  - 沙箱环境支持 C/C++ 源码自动探测编译器（MSVC \`cl.exe\` / \`g++\` / \`clang++\`）；
  - 支持编译标准选择（C++17 推荐 / C++20 / C++14 / C11）与附加编译参数输入；
  - 支持程序标准输入（stdin）管道交互注入与超时（编译 60s / 运行 30s）强杀保护；
  - 严格区分编译错误（带行号高亮）与运行时 stdout/stderr 输出，并在界面清晰标出执行阶段。

---

### 安装包说明
| 平台 | 文件名 | 适用系统与安装方式 |
| :--- | :--- | :--- |
| **Windows (x64)** | \`Noesis_0.1.2_x64-setup.exe\` | Windows 10/11 64 位 NSIS 安装向导（推荐） |
| **Windows (x64)** | \`Noesis_0.1.2_x64_en-US.msi\` | Windows 10/11 64 位 MSI 安装包 |
| **Linux (Debian/Ubuntu amd64)** | \`noesis_0.1.2_amd64.deb\` | Ubuntu / Debian 64 位安装包：\`sudo dpkg -i noesis_0.1.2_amd64.deb\` |
| **Linux (通用 x86_64)** | \`Noesis_0.1.2_linux_x86_64.tar.gz\` | 通用 Linux x86_64 压缩包（含 \`noesis\` ELF 可执行文件与 \`install.sh\`） |
`;

  // 1. Check if release v0.1.2 already exists
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
          name: 'NOESIS v0.1.2 (Windows & Linux)',
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
          name: 'NOESIS v0.1.2 (Windows & Linux)',
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
      name: 'Noesis_0.1.2_x64-setup.exe',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/nsis/Noesis_0.1.2_x64-setup.exe'
      ),
      contentType: 'application/vnd.microsoft.portable-executable',
    },
    {
      name: 'Noesis_0.1.2_x64_en-US.msi',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/msi/Noesis_0.1.2_x64_en-US.msi'
      ),
      contentType: 'application/x-msi',
    },
    {
      name: 'noesis_0.1.2_amd64.deb',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/linux/noesis_0.1.2_amd64.deb'
      ),
      contentType: 'application/vnd.debian.binary-package',
    },
    {
      name: 'Noesis_0.1.2_linux_x86_64.tar.gz',
      filePath: path.join(
        rootDir,
        'src-tauri/target/release/bundle/linux/Noesis_0.1.2_linux_x86_64.tar.gz'
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
