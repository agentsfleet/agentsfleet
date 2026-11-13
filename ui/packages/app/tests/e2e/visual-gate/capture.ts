import {visualProvenance} from "../fixtures/visual-provenance";
import {geometry} from "./geometry";
import {resolve} from "node:path";
import {readFile,writeFile,mkdir,rename} from 'node:fs/promises';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createHash} from 'node:crypto';
import {chromium,expect} from "@playwright/test";
import {applicationCss} from "../fixtures/fleet-output-browser-assets";
import {LIBRARY,LIBRARY_CARD} from "@/tests/helpers/draft-fixtures";
import {VISUAL_SCENARIOS, visualTaskPage,visualTaskDetail} from "../fixtures/refresh-visual-data";
import {LONG_SECRET_NAMES} from "../fixtures/visual-fixture-data";
const app=resolve(import.meta.dirname,"../../..");
const repository=resolve(app,"../../..");
const output=resolve(Bun.argv[2] ?? "visual-gate-results");
const chromiumPath=Bun.argv[3];
const proposal=resolve(Bun.argv[4]??resolve(repository,"../artifacts/fleet-refresh-v2/proposal.html"));
const proposalSha256="8feabc24f6d6e91d2caa22a4b52060074f2877b291fce4d2f1da5c3fc6d8be5a";
const proposalDigest=async()=>createHash("sha256").update(await readFile(proposal)).digest("hex");
if(await proposalDigest()!==proposalSha256)throw new Error("Approved proposal changed");
const fontBase64=(await readFile(proposal,"utf8")).match(/data:font\/woff2;base64,([A-Za-z0-9+/=]+)/)?.[1];
if(!fontBase64)throw new Error("Proposal font is missing");
const referenceFont=Uint8Array.from(atob(fontBase64),char=>char.charCodeAt(0));
const fontPath=Bun.resolveSync("@fontsource-variable/manrope/files/manrope-latin-wght-normal.woff2",resolve(app,"../design-system"));
const fontAssets={proposalSha256:createHash("sha256").update(referenceFont).digest("hex"),appSha256:createHash("sha256").update(await readFile(fontPath)).digest("hex")};
await mkdir(output,{recursive:true});
const sourceFingerprint=visualProvenance;
const startingSource=await sourceFingerprint();

const css=await applicationCss(app);
const build=await promisify(execFile)('bun',['tests/e2e/fixtures/chat-browser-build.ts'],{cwd:app,maxBuffer:30_000_000,timeout:30000});
const html=`<html data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script type="module">${build.stdout.replaceAll('</script','<\\/script')}</script></body></html>`;
const captureNow=Date.parse('2026-11-12T11:44:00Z');
const errors:string[]=[];
const metrics:unknown[]=[];
const server=Bun.serve({hostname:"127.0.0.1",port:0,idleTimeout:0,fetch(request) {
 const url=new URL(request.url);
 const ref=new URL(request.headers.get("referer")??"http://localhost");
 if(url.pathname==="/theme-bootstrap")return new Response("<html><body>Theme bootstrap</body></html>",{headers:{"content-type":"text/html"}});
 if(url.pathname.includes("/events/stream"))return new Response(new ReadableStream({start(controller){controller.enqueue(new TextEncoder().encode('event: hello\ndata: {"kind":"hello","fleet_ids":[]}\n\n'));}}),{headers:{"content-type":"text/event-stream"}});
 if(url.pathname.includes("/fleet-libraries")){
  const labels=ref.searchParams.has("parity")?["Release checklist","Deploy status","Support handoff"]:["Release watch","Deploy status","Support handoff"];
  const cards=labels.map((name,index)=>({...LIBRARY_CARD,id:index===0?LIBRARY_CARD.id:`sample-plugin-${index}`,name,
   description:["Build, scan, and prepare a release for approval.","Check a service and report what needs attention.","Turn a support conversation into a useful handoff."][index]??"",visibility:"tenant",created_at:"2026-11-11T10:00:00Z"}));
  const card=cards[0];
  return Response.json(url.pathname.endsWith("/revisions")?{items:[{...LIBRARY.head,created_at:"2026-11-11T10:00:00Z"}],next_cursor:null}
   :url.pathname.endsWith("/fleet-libraries")||url.pathname.endsWith("/owned")?{items:cards,next_cursor:null}:{...LIBRARY,card});
 }
 if(url.pathname.includes("/tasks")){
  const scenario=VISUAL_SCENARIOS.find(value=>value===ref.searchParams.get("state"))??"completed";
  return Response.json(url.searchParams.has("limit")?visualTaskPage(scenario,captureNow,ref.searchParams.has("parity")):visualTaskDetail(scenario,captureNow,ref.searchParams.has("parity")));
 }
 if(url.pathname.includes("/live/"))return Response.json({items:[],next_cursor:null});
 return new Response(html,{headers:{"content-type":"text/html"}});
}});
const base=server.url.origin;
const browser=await chromium.launch({headless:true,...(chromiumPath?{executablePath:chromiumPath}:{})});
try {
 for (const mode of ['parity','real-data']) for (const touch of [false,true]) for (const width of touch?[390]:[1480,900,696,390]) for (const theme of ['dark','light']) {
  const context=await browser.newContext({deviceScaleFactor:1,reducedMotion:'reduce',hasTouch:touch,isMobile:touch});
  const page=await context.newPage();await page.clock.setFixedTime(new Date(captureNow));page.on('pageerror',e=>errors.push(e.message));page.on('console',event=>{if(event.type()==='error')errors.push(event.text());});
  const pluginName=mode==='parity'?'Release checklist':'Release watch';
  const secretName=mode==='parity'?'REGISTRY_TOKEN':'Release service';
  await page.setViewportSize({width,height:width===390?844:1000});await page.goto(`${base}/theme-bootstrap`);await page.evaluate(t=>localStorage.setItem('theme',t),theme);
  const open=async(query:string)=>{await page.goto(`${base}/?${query}${mode==='parity'?'&parity=1':''}`);await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);await page.evaluate(()=>document.fonts.ready);};
  const snap=async(name:string)=>{
   await page.evaluate(()=>document.fonts.ready);if(!name.includes('hover'))await page.mouse.move(0,0);if(!name.includes('focus'))await page.evaluate(()=>{const a=document.activeElement;if(a instanceof HTMLTextAreaElement&&!a.closest('[role=dialog]'))a.blur();});await page.waitForTimeout(150);
   const data=await page.evaluate(()=>({dialogs:[...document.querySelectorAll('[role=dialog],[role=alertdialog]')].map(n=>{const r=n.getBoundingClientRect();const active=n.querySelector('input:focus');const c=active?getComputedStyle(active):null;return{title:n.querySelector('h2')?.textContent,width:r.width,height:r.height,activeField:active?.getAttribute('aria-label')??active?.getAttribute('type'),focusVisible:active?.matches(':focus-visible'),shadow:c?.boxShadow,padding:c?.paddingLeft};}),fields:[...document.querySelectorAll('input,[role=combobox]')].filter(n=>n.getBoundingClientRect().height>0 && n.getAttribute('aria-hidden')!=='true').map(n=>({role:n.getAttribute('role'),font:getComputedStyle(n).fontSize,padding:getComputedStyle(n).paddingLeft})),overflow:document.documentElement.scrollWidth>innerWidth,headings:[...document.querySelectorAll('h1,h2')].filter(n=>n.getBoundingClientRect().height).map(n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n);return{text:n.textContent,x:r.x,y:r.y,width:r.width,height:r.height,font:s.fontFamily,size:s.fontSize,line:s.lineHeight};}),panels:[...document.querySelectorAll('[aria-label="Period totals"], [aria-label="Daily spend"], .fleet-summary, [data-testid="fleet-reply-figures"]')].map(n=>{const r=n.getBoundingClientRect(),c=getComputedStyle(n);return{label:n.getAttribute('aria-label')??n.className,x:r.x,y:r.y,width:r.width,height:r.height,borderLeft:c.borderLeftWidth,borderRight:c.borderRightWidth};})}));
   if(await page.locator('[role=dialog],[role=alertdialog]').count())await page.keyboard.press('Shift');
   await page.evaluate(()=>{for(const animation of document.getAnimations()){if(animation.effect?.getComputedTiming().iterations!==Infinity)animation.finish();}});
   const measured=await geometry(page);
   await page.screenshot({path:`${output}/${mode}-${name}-${width}-${theme}${touch?"-touch":""}.png`,animations:'disabled'});metrics.push({...startingSource,mode,touch,captureFilename:`${mode}-${name}-${width}-${theme}${touch?"-touch":""}.png`,captureTimeUtc:new Date().toISOString(),name,width,theme,...data,fontsLoaded:await page.evaluate(()=>[400,500,600,650].every(weight=>document.fonts.check(`${weight} 13px "Manrope Variable"`))),geometry:measured});console.log(name,width,theme,data.overflow?'OVERFLOW':'');
  };
  const snapBottom=async(name:string)=>{
   const scroller=page.locator('.app-dashboard-canvas > div:not([data-resource-header])').first();
   if(!await scroller.count())return;
   const state=await scroller.evaluate(n=>({top:n.scrollTop,extra:n.scrollHeight-n.clientHeight}));
   if(state.extra<=1)return;
   await scroller.evaluate(n=>n.scrollTop=n.scrollHeight);await snap(`${name}-bottom`);
   await scroller.evaluate((n,top)=>n.scrollTop=top,state.top);
  };
  await open('resources=1'); await expect(page.getByRole('heading',{name:'Models',exact:true})).toBeVisible();await snap('models');await snapBottom('models');
  await page.getByRole('button',{name:'Create model',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();await snap('create-model-new-key');if(width===390){const d=page.getByRole('dialog');await d.evaluate(n=>n.scrollTop=n.scrollHeight);await snap('create-model-new-key-bottom');await d.evaluate(n=>n.scrollTop=0);}
  await page.getByRole('radio',{name:/saved key/i}).click();await snap('create-model-saved-key');await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.getByRole('button',{name:/^Manage /}).first().click();await page.getByRole('menuitem',{name:'Model details',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();await snap('model-details');await page.getByRole('button',{name:'Close',exact:true}).click();
  await page.getByRole('button',{name:/^Manage /}).first().click();await page.getByRole('menuitem',{name:'Edit model',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();await snap('edit-model');await page.getByRole('button',{name:'Cancel',exact:true}).click();
  if(width===1480||width===390){await page.getByRole('button',{name:'Create model',exact:true}).hover();await snap('models-primary-hover');await page.mouse.move(0,0);await page.getByRole('button',{name:'Create model',exact:true}).focus();await page.keyboard.press('Shift+Tab');await page.keyboard.press('Tab');await snap('models-primary-focus');}

  await page.getByRole('button',{name:'Team',exact:true}).click();await expect(page.getByRole('heading',{name:'Team',exact:true})).toBeVisible();await snap('team');await snapBottom('team');
  await page.getByRole('button',{name:'Invite member',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();await snap('invite-member');await page.getByRole('button',{name:'Cancel',exact:true}).click();
  if(width===1480||width===390){await page.getByRole('searchbox').focus();await page.keyboard.press('Shift+Tab');await page.keyboard.press('Tab');await snap('team-search-focus');}

  await open('accountSettings=1');await expect(page.getByText('Chrome on macOS')).toBeVisible();await snap('security');await snapBottom('security');
  await page.getByRole('button',{name:'Create API key',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();await snap('create-api-key');await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.getByRole('button',{name:/Sign out Safari/}).click();await expect(page.getByRole('alertdialog')).toBeVisible();await snap('sign-out-session');await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.getByRole('button',{name:'Revoke API key Deploy automation',exact:true}).click();await expect(page.getByRole('alertdialog')).toBeVisible();await snap('revoke-api-key');await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.getByRole('button',{name:'Sign out other sessions',exact:true}).click();await expect(page.getByRole('alertdialog')).toBeVisible();await snap('sign-out-other-sessions');await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.getByRole('button',{name:'Errors',exact:true}).click();await expect(page.getByText('Sessions could not be loaded. Try again.')).toBeVisible();await snap('security-error');

  await open('resourcePages=1');await expect(page.getByRole('heading',{name:'Usage',exact:true})).toBeVisible();await snap('usage');await snapBottom('usage');
  for (const [label,name] of [['Secrets','secrets'],['Plugins','plugins'],['Connected apps','connected-apps'],['New fleet','new-fleet']] as const) {
   await page.locator('[data-review-pages]').getByRole('button',{name:label,exact:true}).click();
   if(name==='plugins') await expect(page.getByRole('button',{name:`Manage ${pluginName}`})).toBeVisible();
   if(name==='new-fleet') await expect(page.getByRole('textbox')).toBeVisible();
   await snap(name);if(name!=='new-fleet')await snapBottom(name);
   if(name==='secrets'){
    await page.getByRole('button',{name:'Add secret',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();await snap('add-secret');await page.getByRole('button',{name:'Cancel',exact:true}).click();
    const manage=page.getByRole('button',{name:`Manage ${secretName}`,exact:true});await manage.scrollIntoViewIfNeeded();if(width===390)await snap('secrets-scrolled-actions');
    await manage.click();await expect(page.getByRole('dialog')).toBeVisible();await snap('manage-secret');await page.getByRole('button',{name:'Replace value',exact:true}).click();await expect(page.getByRole('dialog',{name:/^Replace secret/})).toBeVisible();await expect(page.getByLabel('Field 1 value',{exact:true})).toBeVisible();await expect(page.locator('[role=dialog] input:focus')).toHaveCount(1);await snap('replace-secret');await page.getByRole('button',{name:'Cancel',exact:true}).click();
    await manage.click();await page.getByRole('dialog').getByRole('button',{name:'Delete secret',exact:true}).click();await expect(page.getByRole('alertdialog')).toBeVisible();await snap('delete-secret');await page.getByRole('button',{name:'Cancel',exact:true}).click();
   }
   if(name==='plugins'){await page.getByRole('button',{name:`Manage ${pluginName}`}).click();await expect(page.getByRole('button',{name:'Use for new fleet'})).toBeVisible();await snap('plugin-details');await page.getByRole('button',{name:'Close',exact:true}).click();}
   if(name==='connected-apps'){await page.getByRole('button',{name:'Manage GitHub'}).click();await expect(page.getByRole('dialog')).toBeVisible();await snap('connected-app-details');await page.getByRole('button',{name:'Done',exact:true}).click();}

  }
  for(const [state,name] of [['completed','conversation'],['activity','activity'],['wake-up','wake-up'],['error','error'],['running','working']] as const){
   await open(`visual=1&state=${state}`);await expect(page.getByRole('textbox')).toBeVisible();
   if(state==='completed') await expect(page.getByRole('button',{name:'Approve',exact:true})).toBeVisible();
   if(state==='activity'){const worked=page.getByRole('button',{name:/Worked/}).first();if(await worked.count())await worked.click();}
   await snap(name);if(state==='activity'){await page.getByRole('button',{name:/^3 files changed$/}).click();await expect(page.getByRole('dialog',{name:'Recorded changes'})).toBeVisible();await snap('recorded-changes');await page.getByRole('button',{name:'Close',exact:true}).click();}if(['completed','wake-up','running'].includes(state)){const viewport=page.locator('[role="presentation"]').filter({has:page.getByRole('textbox')});if(await viewport.evaluate(node=>node.scrollTop)>0){await viewport.evaluate(node=>node.scrollTop=0);await snap(`${name}-top`);}}if(state==='completed'){
    await page.getByRole('textbox').focus();await snap('conversation-composer-focus');
    await page.getByRole('button',{name:'View exact action',exact:true}).click();await expect(page.getByRole('dialog',{name:'Exact action'})).toBeVisible();await snap('approval-details');await page.getByRole('button',{name:'Done',exact:true}).click();
    for(const [label,name] of [['Archive','archive-fleet'],['Permanently delete','delete-fleet']] as const){
      await page.getByRole('button',{name:'Fleet actions',exact:true}).click();
      await page.getByRole('menuitem',{name:label,exact:true}).click();
      await expect(page.getByRole('alertdialog')).toBeVisible();await snap(name);
      await page.getByRole('button',{name:'Cancel',exact:true}).click();
    }
    await open('visual=1&state=completed&archived=1');await expect(page.getByRole('textbox')).toHaveCount(0);await expect(page.getByRole('button',{name:'Approve',exact:true})).toHaveCount(0);await expect(page.getByText('Archived',{exact:true}).first()).toBeVisible();await snap('fleet-archived');
    await open('resourcePages=1&resource=new&notice=deleted');await expect(page.getByRole('textbox')).toBeVisible();await expect(page.getByRole('heading',{name:'New fleet',exact:true})).toBeVisible();await snap('fleet-deleted-destination');
    await open('visual=1&state=completed');await expect(page.getByRole('button',{name:'Approve',exact:true})).toBeVisible();await page.getByRole('button',{name:/^Workspace and account menu/}).click();await expect(page.getByRole('menu')).toBeVisible();await snap('account-menu');await page.keyboard.press('Escape');
  }
  }
  if(width===1480||width===390){
   for(const [kind,name]of Object.entries(LONG_SECRET_NAMES)){
    await open(`resourcePages=1&resource=secrets&longSecret=${kind}`);await page.getByRole('button',{name:`Manage ${name}`,exact:true}).click();await snap(`manage-secret-long-${kind}`);
    await page.getByRole('button',{name:'Replace value',exact:true}).click();await expect(page.getByLabel('Field 1 value',{exact:true})).toBeVisible();await snap(`replace-secret-long-${kind}`);await page.getByRole('button',{name:'Cancel',exact:true}).click();
    await page.getByRole('button',{name:`Manage ${name}`,exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'Delete secret',exact:true}).click();await snap(`delete-secret-long-${kind}`);await page.getByRole('button',{name:'Cancel',exact:true}).click();
   }
   for(const outcome of ['archived','deleted']){
    await open(outcome==='archived'?'visual=1&state=completed&archived=1':'resourcePages=1&resource=new&notice=deleted');
    await page.addStyleTag({content:'.fixed.inset-x-0.top-0{display:none!important}.app-shell{top:0!important;height:100dvh!important}'});await snap(`fleet-${outcome}-without-review-bar`);
   }
  }
  if(width===390){await page.setViewportSize({width,height:500});await open('visual=1&state=running');await page.getByRole('textbox').focus();await snap('working-reduced-viewport-focus');}
  await context.close();
 }
 const recordings=[];
 for(const theme of ['dark','light']) for(const width of [1480,390]) for(const reducedMotion of ['no-preference','reduce'] as const){
   const prefix=`loader-${width}-${theme}-${reducedMotion}`;
   const recording=await browser.newContext({viewport:{width,height:width===390?844:1000},deviceScaleFactor:1,reducedMotion,recordVideo:{dir:output,size:{width,height:width===390?844:1000}}});
   const motion=await recording.newPage();await motion.clock.setFixedTime(new Date(captureNow));await motion.goto(`${base}/theme-bootstrap`);await motion.evaluate(t=>localStorage.setItem('theme',t),theme);await motion.goto(`${base}/?visual=1&state=running`);await motion.evaluate(t=>document.documentElement.dataset.theme=t,theme);await motion.evaluate(()=>document.fonts.ready);
   const label=motion.getByRole('button',{name:/^Tool activity:/}).locator('[data-chat-loading="true"]').first();await expect(label).toBeVisible();await label.scrollIntoViewIfNeeded();
   const labelBox=await label.boundingBox();
   const styles=()=>label.evaluate(n=>{const c=getComputedStyle(n);return {text:n.textContent,animation:c.animationName,position:c.backgroundPosition,color:c.color,gradient:c.backgroundImage,base:c.getPropertyValue("--chat-loading-base")};});
   const before=await styles();await label.screenshot({path:`${output}/${prefix}-frame-0.png`});await motion.waitForTimeout(700);await label.screenshot({path:`${output}/${prefix}-frame-1.png`});const after=await styles();
   await motion.waitForTimeout(8300);const video=motion.video();await recording.close();if(video)await rename(await video.path(),`${output}/${prefix}.webm`);recordings.push({...startingSource,captureTimeUtc:new Date().toISOString(),width,theme,reducedMotion,labelBox,before,after});console.log('recorded',prefix);
 }
 await writeFile(`${output}/loader-measurements.json`,JSON.stringify({...startingSource,recordings},null,2));
 if(await proposalDigest()!==proposalSha256)throw new Error("Proposal changed during capture");
 const endingSource=await sourceFingerprint();if(endingSource.revision!==startingSource.revision || endingSource.sourceTreeSha256!==startingSource.sourceTreeSha256)throw Error('Source changed during capture; do not publish this run.');
 const revision=(await promisify(execFile)('git',['rev-parse','HEAD'],{cwd:app})).stdout.trim();
 const changes=(await promisify(execFile)('git',['diff','HEAD','--','.',':(exclude)ui/packages/app/next-env.d.ts'],{cwd:repository,maxBuffer:5_000_000})).stdout;
 await writeFile(`${output}/measurements.json`,JSON.stringify({...startingSource,revision,captureTimeUtc:new Date().toISOString(),captureScriptSha256:createHash('sha256').update(await readFile(import.meta.path)).digest('hex'),sourceFingerprint:startingSource,endingSource,workingTreeDiffSha256:changes?createHash('sha256').update(changes).digest('hex'):null,reference:'8feabc24f6d6e91d2caa22a4b52060074f2877b291fce4d2f1da5c3fc6d8be5a',sampleTime:new Date(captureNow).toISOString(),fontAssets,matrix:{modes:["parity","real-data"],widths:[1480,900,696,390],themes:["dark","light"],touchWidths:[390]},metrics,errors,recordings},null,2));console.log(JSON.stringify({errors,captures:metrics.length}));
} finally {await browser.close();await server.stop(true);}
