// Run against the local Vite development server. Requires Playwright and Chromium.
const {chromium} = require('playwright');
const path = require('node:path');
const fs = require('node:fs/promises');
const base = process.env.SCREENSHOT_URL || 'http://127.0.0.1:24208';
const output = path.resolve(__dirname, '../images/product');
(async () => {
  const browser = await chromium.launch({headless:true, ...(process.env.CHROME_PATH ? {executablePath:process.env.CHROME_PATH} : {})});
  try {
    await fs.mkdir(output, {recursive:true});
    for (const lang of ['zh','en']) {
      const page = await browser.newPage({viewport:{width:1440,height:900},deviceScaleFactor:1.5});
      const errors = [];
      page.on('pageerror', error=>errors.push(error.message));
      // Screenshots need no network data. Block accidental outbound requests.
      await page.route('**/*', route=>new URL(route.request().url()).origin === new URL(base).origin ? route.continue() : route.abort());
      for (const scene of ['library','document','install','tasks','import','review','errors','log']) {
        await page.goto(`${base}/docs/screenshots/index.html?lang=${lang}&scene=${scene}`);
        if (['install','tasks','import','review','errors','log'].includes(scene)) await page.getByRole('dialog').waitFor();
        else await page.getByRole('button',{name:'example/research-kit',exact:true}).waitFor();
        if (scene==='library' || scene==='document') {
          for (const repo of ['research-kit','dev-toolkit']) await page.getByRole('button',{name:lang==='zh'?`展开 example/${repo}`:`Expand example/${repo}`,exact:true}).click();
          if (scene==='library') await page.getByRole('button',{name:'example/research-kit',exact:true}).click();
          else {
            await page.getByRole('button',{name:lang==='zh'?'查看 literature-review 的详情':'View details for literature-review',exact:true}).first().click();
            await page.locator('h1').filter({hasText:lang==='zh'?'文献综述':'Literature review'}).waitFor();
          }
        }
        if (scene==='import') await page.getByRole('button',{name:lang==='zh'?'选择技能':'Choose skills',exact:true}).click();
        if (scene==='install') {
          await page.getByRole('checkbox',{name:'Claude Code',exact:true}).check();
          await page.getByRole('button',{name:lang==='zh'?'安装到 1 个目标':'Install to 1 target',exact:true}).waitFor();
          await page.getByRole('checkbox',{name:'Website',exact:true}).check();
          await page.getByRole('button',{name:lang==='zh'?'安装到 2 个目标':'Install to 2 targets',exact:true}).waitFor();
        }
        if (scene==='tasks') await page.getByRole('article').filter({hasText:lang==='zh'?'更新技能仓库':'Update skill library'}).getByRole('button').first().click();
        if (scene==='review') {
          await page.getByRole('region',{name:'example/research-kit',exact:true}).getByRole('button',{name:lang==='zh'?'查看详情':'View details',exact:true}).click();
          await page.locator('summary').filter({hasText:'literature-review'}).click();
        }
        if (scene==='errors') await page.getByRole('button',{name:lang==='zh'?'需重查 1':'Needs recheck 1',exact:true}).click();
        if (scene==='log') await page.getByRole('tab',{name:lang==='zh'?'日志':'Log',exact:true}).click();
        await page.evaluate(()=>document.fonts.ready);
        await page.screenshot({path:path.join(output,`${scene}-${lang}.png`),animations:'disabled'});
        console.log(`${scene}-${lang}.png`);
      }
      if (errors.length) throw new Error(errors.join('\n'));
      await page.close();
    }
  } finally { await browser.close(); }
})().catch(error=>{console.error(error);process.exitCode=1;});
