import fs from 'fs';
import path from 'path';

const suitePath = path.resolve(process.cwd(), 'eval/benchmark_suite.json');
const suite = JSON.parse(fs.readFileSync(suitePath, 'utf-8'));

console.log(`\n======================================================`);
console.log(`🚀 NOESIS 解读质量评测流水线 (Eval Benchmark Harness)`);
console.log(`   版本: ${suite.version} | 骨架用例数: ${suite.cases.length}`);
console.log(`   状态: [框架已就绪 / 真实模型对比基线打分待接入 (阶段 C)]`);
console.log(`======================================================\n`);

let totalPoints = 0;

for (const tc of suite.cases) {
  console.log(`▶ [${tc.perspective.toUpperCase()}] 用例 ${tc.id}`);
  console.log(`  目标源码: ${tc.target_file}`);
  console.log(`  问题: ${tc.question}`);

  // Check if target file exists
  const targetExists = fs.existsSync(path.resolve(process.cwd(), tc.target_file));
  console.log(`  源码存在性: ${targetExists ? '✅ 文件存在' : '❌ 文件不存在'}`);

  console.log(`  评估参考核心要点 (${tc.reference_points.length} 点待评测):`);
  for (const ref of tc.reference_points) {
    totalPoints++;
    console.log(`    • ${ref}`);
  }
  console.log('');
}

console.log(`------------------------------------------------------`);
console.log(`📊 评测准备状态:`);
console.log(`   总核心要点: ${totalPoints}`);
console.log(`   执行模式:   基线框架脚手架 (待与通用 AI 盲评对比实测)`);
console.log(`   说明:       真实评测流水线将在接入 20+ 道实测题目后正式启动`);
console.log(`======================================================\n`);
