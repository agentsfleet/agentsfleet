import {visualProvenance} from "../fixtures/visual-provenance";
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {chromium} from "@playwright/test";
import {geometry} from "./geometry";
const source=await visualProvenance();
const reference=Bun.argv[2];
const output=Bun.argv[3];
const chromiumPath=Bun.argv[4];
if(!reference||!output)throw new Error("Usage: reference.ts <proposal.html> <output directory> [Chromium executable]");
await mkdir(`${output}/reference`,{recursive:true});
const expected='8feabc24f6d6e91d2caa22a4b52060074f2877b291fce4d2f1da5c3fc6d8be5a';
const digest=createHash('sha256').update(await readFile(reference)).digest('hex');
if(digest!==expected)throw Error('Approved proposal changed');
const browser=await chromium.launch({headless:true,...(chromiumPath?{executablePath:chromiumPath}:{})});
const results=[];
try {
 const page=await browser.newPage({deviceScaleFactor:1});
 for(const width of [1480,390,696,900])for(const theme of ['dark','light'])for(const view of ['conversation','activity','scheduled','failure','empty','models','usage','security','team','secrets','plugins','connections']){
  await page.setViewportSize({width,height:width===390?844:1000});
  await page.goto(`file://${reference}`);
  await page.evaluate(({view,theme})=>{document.documentElement.dataset.theme=theme;const showView:unknown=Reflect.get(window,"showView");if(typeof showView!=="function")throw new Error("Missing proposal view action");showView(view);},{view,theme});
  await page.evaluate(()=>document.fonts.ready);
  const captureFilename=`reference/${view}-${width}-${theme}.png`;
  await page.mouse.move(0,0);await page.screenshot({path:`${output}/${captureFilename}`,animations:'disabled'});
  results.push({...source,captureTimeUtc:new Date().toISOString(),width,theme,view,captureFilename,geometry:await geometry(page)});
  const actions:Record<string,string[]>={models:['[data-action=create-model]'],team:['[data-team-action=invite]'],security:['[data-action=create-key]','[data-revoke-key]','[data-revoke-session]','[data-action=revoke-other-sessions]'],secrets:['[data-action=add-secret]'],plugins:['[data-plugin]'],activity:['[data-action=activity-changes]'],conversation:['[data-action=changes]'],connections:['[data-action=connection-details]']};
  for(const action of actions[view]??[]){
   const button=page.locator(action).first();
   if(!await button.count())continue;
   await button.click();
   if(await page.locator('dialog[open]').count()){
    const captureFilename=`reference/${view}-dialog-${action.replace(/[^a-z0-9-]/gi,"")}-${width}-${theme}.png`;
    await page.mouse.move(0,0);await page.screenshot({path:`${output}/${captureFilename}`,animations:'disabled'});
    results.push({...source,captureTimeUtc:new Date().toISOString(),width,theme,view:`${view}-dialog-${action}`,captureFilename,geometry:await geometry(page)});
    if(view==='models'){
      await page.getByLabel('Use saved key',{exact:false}).check();
      const savedFilename=`reference/create-model-saved-key-${width}-${theme}.png`;
      await page.screenshot({path:`${output}/${savedFilename}`,animations:'disabled'});
      results.push({...source,captureTimeUtc:new Date().toISOString(),width,theme,view:'create-model-saved-key',captureFilename:savedFilename,geometry:await geometry(page)});
    }
    await page.keyboard.press('Escape');
   }
  }

 }

 await writeFile(`${output}/reference-measurements.json`,JSON.stringify({...source,referenceSha256:digest,captureTimeUtc:new Date().toISOString(),results},null,2));
 console.log(JSON.stringify(results.filter(r=>r.width===1480&&r.theme==='dark').map(r=>({view:r.view,composer:r.geometry.composer,flap:r.geometry.flap,strip:r.geometry.strip})),null,2));
 const finalDigest=createHash('sha256').update(await readFile(reference)).digest('hex');
 if(finalDigest!==expected)throw new Error('Proposal changed during reference render');
 const endingSource=await visualProvenance();if(endingSource.revision!==source.revision||endingSource.sourceTreeSha256!==source.sourceTreeSha256)throw new Error('Source changed during reference render');
}finally{await browser.close();}
