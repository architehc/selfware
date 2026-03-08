#!/usr/bin/env node
/**
 * Selfware Report Aggregator
 * 
 * Aggregates QA reports from multiple languages into a unified report.
 * Also provides utilities for report transformation and analysis.
 * 
 * Usage:
 *   node report-aggregator.js --input reports/ --output unified-report.json
 *   node report-aggregator.js --analyze unified-report.json --format markdown
 */

const fs = require('fs').promises;
const path = require('path');

// Quality scoring weights
const WEIGHTS = {
  syntax: 0.10,
  format: 0.05,
  lint: 0.15,
  typecheck: 0.10,
  test: 0.30,
  coverage: 0.20,
  security: 0.10,
};

// Grade thresholds
const GRADE_THRESHOLDS = {
  S: 95,
  A: 90,
  B: 80,
  C: 70,
  D: 60,
  F: 0,
};

/**
 * Calculate grade from score
 */
function calculateGrade(score) {
  for (const [grade, threshold] of Object.entries(GRADE_THRESHOLDS).sort((a, b) => b[1] - a[1])) {
    if (score >= threshold) return grade;
  }
  return 'F';
}

/**
 * Calculate quality score from stage results
 */
function calculateQualityScore(stages) {
  const scores = {};
  
  // Calculate individual stage scores
  for (const stage of stages) {
    const name = stage.name;
    
    switch (name) {
      case 'syntax':
      case 'format':
      case 'typecheck':
        scores[name] = stage.status === 'passed' ? 100 : 0;
        break;
        
      case 'lint':
        if (stage.status === 'passed') {
          scores[name] = 100;
        } else {
          const errorCount = stage.output?.error_count || 5;
          scores[name] = Math.max(0, 100 - errorCount * 5);
        }
        break;
        
      case 'test':
        if (stage.output?.total > 0) {
          scores[name] = (stage.output.passed / stage.output.total) * 100;
        } else {
          scores[name] = 0;
        }
        break;
        
      case 'coverage':
        const coverage = stage.output?.percentage || 0;
        scores[name] = coverage < 80 ? (coverage / 80) * 100 : 100;
        break;
        
      case 'security':
        const critical = stage.output?.critical || 0;
        const high = stage.output?.high || 0;
        const medium = stage.output?.medium || 0;
        scores[name] = Math.max(0, 100 - critical * 50 - high * 20 - medium * 5);
        break;
    }
  }
  
  // Calculate weighted total
  let total = 0;
  let totalWeight = 0;
  
  for (const [key, weight] of Object.entries(WEIGHTS)) {
    if (scores[key] !== undefined) {
      total += scores[key] * weight;
      totalWeight += weight;
    }
  }
  
  // Normalize if not all stages present
  if (totalWeight > 0) {
    total = (total / totalWeight) * Object.values(WEIGHTS).reduce((a, b) => a + b, 0);
  }
  
  return {
    overall: Math.round(total * 10) / 10,
    breakdown: scores,
    grade: calculateGrade(total),
    passed: total >= 80 && (scores.security || 100) >= 70,
  };
}

/**
 * Load and parse a report file
 */
async function loadReport(filePath) {
  try {
    const content = await fs.readFile(filePath, 'utf-8');
    return JSON.parse(content);
  } catch (error) {
    console.error(`Error loading report ${filePath}:`, error.message);
    return null;
  }
}

/**
 * Discover report files in a directory
 */
async function discoverReports(reportsDir) {
  const reports = [];
  
  try {
    const entries = await fs.readdir(reportsDir, { withFileTypes: true });
    
    for (const entry of entries) {
      if (entry.isFile() && entry.name.endsWith('-report.json')) {
        const language = entry.name.replace('-report.json', '');
        const filePath = path.join(reportsDir, entry.name);
        const report = await loadReport(filePath);
        
        if (report) {
          reports.push({ language, report, filePath });
        }
      }
    }
  } catch (error) {
    console.error(`Error reading reports directory:`, error.message);
  }
  
  return reports;
}

/**
 * Aggregate multiple reports into a unified report
 */
async function aggregateReports(inputDir, outputPath) {
  console.log(`Aggregating reports from ${inputDir}...`);
  
  const reports = await discoverReports(inputDir);
  
  if (reports.length === 0) {
    console.warn('No reports found to aggregate');
    return null;
  }
  
  console.log(`Found ${reports.length} reports: ${reports.map(r => r.language).join(', ')}`);
  
  // Collect all stages
  const allStages = [];
  const languages = [];
  
  for (const { language, report } of reports) {
    languages.push(language);
    
    if (report.stages) {
      for (const stage of report.stages) {
        allStages.push({
          ...stage,
          language,
        });
      }
    }
  }
  
  // Calculate quality score
  const scoreData = calculateQualityScore(allStages);
  
  // Build unified report
  const unifiedReport = {
    report_version: '1.0',
    timestamp: new Date().toISOString(),
    project: 'generated-code',
    languages,
    summary: {
      total_reports: reports.length,
      total_stages: allStages.length,
      passed_stages: allStages.filter(s => s.status === 'passed').length,
      failed_stages: allStages.filter(s => s.status === 'failed').length,
      skipped_stages: allStages.filter(s => s.status === 'skipped').length,
    },
    stages: allStages,
    quality_score: scoreData.overall,
    grade: scoreData.grade,
    passed: scoreData.passed,
    score_breakdown: scoreData.breakdown,
  };
  
  // Save unified report
  await fs.writeFile(outputPath, JSON.stringify(unifiedReport, null, 2));
  console.log(`Unified report saved to ${outputPath}`);
  
  // Print summary
  console.log('\n=== Quality Summary ===');
  console.log(`Quality Score: ${unifiedReport.quality_score}/100`);
  console.log(`Grade: ${unifiedReport.grade}`);
  console.log(`Passed: ${unifiedReport.passed ? '✅' : '❌'}`);
  console.log('\nScore Breakdown:');
  for (const [key, value] of Object.entries(scoreData.breakdown)) {
    console.log(`  ${key}: ${value.toFixed(1)}`);
  }
  
  return unifiedReport;
}

/**
 * Generate markdown report from unified report
 */
async function generateMarkdownReport(reportPath, outputPath) {
  const report = await loadReport(reportPath);
  
  if (!report) {
    console.error('Failed to load report');
    return;
  }
  
  const gradeEmoji = {
    S: '🏆',
    A: '✨',
    B: '✅',
    C: '⚠️',
    D: '❗',
    F: '❌',
  };
  
  let markdown = `# Selfware QA Report\n\n`;
  markdown += `**Generated:** ${report.timestamp}\n\n`;
  markdown += `**Languages:** ${report.languages.join(', ')}\n\n`;
  
  // Quality Score
  markdown += `## Quality Score\n\n`;
  markdown += `# ${gradeEmoji[report.grade] || ''} ${report.quality_score}/100 (${report.grade} Grade)\n\n`;
  markdown += `**Status:** ${report.passed ? '✅ PASSED' : '❌ FAILED'}\n\n`;
  
  // Score Breakdown
  markdown += `### Score Breakdown\n\n`;
  markdown += `| Category | Score | Weight | Weighted |\n`;
  markdown += `|----------|-------|--------|----------|\n`;
  
  for (const [key, score] of Object.entries(report.score_breakdown || {})) {
    const weight = WEIGHTS[key] || 0;
    const weighted = (score * weight).toFixed(1);
    markdown += `| ${key} | ${score.toFixed(1)}% | ${(weight * 100).toFixed(0)}% | ${weighted} |\n`;
  }
  
  markdown += `\n`;
  
  // Summary
  markdown += `## Summary\n\n`;
  markdown += `- **Total Stages:** ${report.summary.total_stages}\n`;
  markdown += `- **Passed:** ${report.summary.passed_stages} ✅\n`;
  markdown += `- **Failed:** ${report.summary.failed_stages} ❌\n`;
  markdown += `- **Skipped:** ${report.summary.skipped_stages} ⏭️\n`;
  markdown += `\n`;
  
  // Stage Details
  markdown += `## Stage Details\n\n`;
  
  const stagesByName = {};
  for (const stage of report.stages || []) {
    if (!stagesByName[stage.name]) {
      stagesByName[stage.name] = [];
    }
    stagesByName[stage.name].push(stage);
  }
  
  for (const [name, stages] of Object.entries(stagesByName)) {
    markdown += `### ${name.charAt(0).toUpperCase() + name.slice(1)}\n\n`;
    markdown += `| Language | Status | Duration |\n`;
    markdown += `|----------|--------|----------|\n`;
    
    for (const stage of stages) {
      const statusEmoji = stage.status === 'passed' ? '✅' : 
                         stage.status === 'failed' ? '❌' : '⏭️';
      const duration = stage.duration_ms ? `${stage.duration_ms}ms` : '-';
      markdown += `| ${stage.language} | ${statusEmoji} ${stage.status} | ${duration} |\n`;
    }
    
    markdown += `\n`;
  }
  
  // Save markdown
  await fs.writeFile(outputPath, markdown);
  console.log(`Markdown report saved to ${outputPath}`);
}

/**
 * Analyze a report and provide insights
 */
async function analyzeReport(reportPath) {
  const report = await loadReport(reportPath);
  
  if (!report) {
    console.error('Failed to load report');
    return;
  }
  
  console.log('\n=== Report Analysis ===\n');
  
  // Identify problem areas
  const failedStages = report.stages.filter(s => s.status === 'failed');
  
  if (failedStages.length === 0) {
    console.log('✅ All stages passed!');
  } else {
    console.log(`❌ ${failedStages.length} stage(s) failed:\n`);
    
    for (const stage of failedStages) {
      console.log(`  - ${stage.name} (${stage.language})`);
      
      if (stage.output?.error) {
        console.log(`    Error: ${stage.output.error}`);
      }
    }
  }
  
  // Coverage analysis
  const coverageStage = report.stages.find(s => s.name === 'coverage');
  if (coverageStage) {
    const coverage = coverageStage.output?.percentage || 0;
    console.log(`\n📊 Coverage: ${coverage.toFixed(1)}%`);
    
    if (coverage < 80) {
      console.log(`   ⚠️ Below target (80%)`);
    }
  }
  
  // Security analysis
  const securityStage = report.stages.find(s => s.name === 'security');
  if (securityStage) {
    const critical = securityStage.output?.critical || 0;
    const high = securityStage.output?.high || 0;
    
    if (critical > 0 || high > 0) {
      console.log(`\n🔒 Security Issues:`);
      console.log(`   Critical: ${critical}`);
      console.log(`   High: ${high}`);
    } else {
      console.log(`\n🔒 No critical/high security issues found`);
    }
  }
  
  // Recommendations
  console.log('\n=== Recommendations ===\n');
  
  const recommendations = [];
  
  if (report.quality_score < 80) {
    recommendations.push('Address failing stages to improve quality score');
  }
  
  if (report.score_breakdown?.coverage < 80) {
    recommendations.push('Increase test coverage to at least 80%');
  }
  
  if (report.score_breakdown?.lint < 100) {
    recommendations.push('Fix linting errors for code quality');
  }
  
  if (report.score_breakdown?.security < 100) {
    recommendations.push('Address security vulnerabilities immediately');
  }
  
  if (recommendations.length === 0) {
    console.log('✅ No recommendations - code meets all quality standards!');
  } else {
    for (let i = 0; i < recommendations.length; i++) {
      console.log(`${i + 1}. ${recommendations[i]}`);
    }
  }
}

/**
 * Main CLI
 */
async function main() {
  const args = process.argv.slice(2);
  
  // Parse arguments
  const options = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    const value = args[i + 1];
    options[key] = value;
  }
  
  if (options.input && options.output) {
    // Aggregate mode
    await aggregateReports(options.input, options.output);
  } else if (options.analyze) {
    // Analyze mode
    await analyzeReport(options.analyze);
  } else if (options.report && options.format === 'markdown') {
    // Generate markdown
    const outputPath = options.output || 'qa-report.md';
    await generateMarkdownReport(options.report, outputPath);
  } else {
    console.log(`
Selfware Report Aggregator

Usage:
  Aggregate reports:
    node report-aggregator.js --input reports/ --output unified-report.json
  
  Analyze report:
    node report-aggregator.js --analyze unified-report.json
  
  Generate markdown:
    node report-aggregator.js --report unified-report.json --format markdown --output report.md
`);
  }
}

// Run main
main().catch(error => {
  console.error('Error:', error);
  process.exit(1);
});
