import type {Page} from "@playwright/test";
export async function measureVisualState(page: Page) {
 return page.evaluate(() => {
  const visible=(node:Element)=>{const r=node.getBoundingClientRect();return r.width>0&&r.height>0&&getComputedStyle(node).visibility!=='hidden'&&!node.closest('[aria-hidden=true],[hidden]');};
  const box=(node:Element|null)=>{
   if(!node)return null;
   const r=node.getBoundingClientRect(),s=getComputedStyle(node);
   return {tag:node.tagName,id:node.id,type:node.getAttribute("type"),name:node.getAttribute("aria-label")??((node as HTMLInputElement).labels?[...(node as HTMLInputElement).labels!].map(label=>label.textContent).join(" "):node.textContent?.trim()),x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom,text:node.textContent?.trim(),role:node.getAttribute('role'),label:node.getAttribute('aria-label'),font:s.fontFamily,fontSize:s.fontSize,fontWeight:s.fontWeight,lineHeight:s.lineHeight,paddingLeft:s.paddingLeft,paddingRight:s.paddingRight,gap:s.gap,border:s.borderTopWidth,borderColor:s.borderTopColor,radius:s.borderTopLeftRadius,background:s.backgroundColor,color:s.color,boxShadow:s.boxShadow,ringWidth:s.getPropertyValue("--tw-ring-shadow"),ringOffset:s.getPropertyValue("--tw-ring-offset-width"),outline:s.outlineWidth,outlineOffset:s.outlineOffset,outlineColor:s.outlineColor,filter:s.filter,focusVisible:node.matches(':focus-visible')};
  };
  const all=(selector:string,root:ParentNode=document)=>[...root.querySelectorAll(selector)].filter(visible);
  const dialogs=all('[role=dialog],[role=alertdialog],dialog[open]').map(node=>({rect:box(node),title:box(node.querySelector('h2')),titleRow:box(node.querySelector('h2')),close:box(node.querySelector('[data-dialog-close],button[aria-label=Close]')),fields:all('input,textarea,select,[role=combobox]',node).map(box),footerButtons:all('[class*=justify-end] button,.modal-actions button',node).map(box),buttons:all('button',node).map(box),descriptionRows:all('dl > div',node).map(n=>({rect:box(n),cells:[...n.children].map(box)})),sourceEvidence:all('a',node).filter(n=>n.textContent?.trim()==='Source').map(n=>({rect:box(n.parentElement),cells:[...n.parentElement!.children].map(box)})),footerContainers:all('[class*=justify-end],.modal-actions',node).filter(n=>n.querySelector(':scope > button')).map(n=>({rect:box(n),previous:box(n.previousElementSibling),buttons:all('button',n).map(box)})),activeElement:node.contains(document.activeElement)?box(document.activeElement):null}));
  const textarea=all('textarea').find(n=>!n.closest('[role=dialog],[role=alertdialog]')&&n.closest('[data-dashboard-panel],.composer'));
  const composer=textarea?.closest('[data-dashboard-panel],.composer,form')??textarea?.closest('[class*=focus-within]');
  const flap=all('button,.work-flap').find(n=>/^(\d+ files? changed|Changes)/.test(n.textContent?.trim()??''));
  const edit=all('button[aria-label="Edit wake-up"]')[0];
  const strip=edit?.closest('[class*=rounded-t]')??all('.wake-strip')[0];
  return {viewport:{width:innerWidth,height:innerHeight},shell:box(document.querySelector(".app-dashboard-canvas,.surface")),dialogs,activeElement:box(document.activeElement),composer:box(composer??null),flap:box(flap??null),strip:box(strip??null),fields:all('input,textarea,select,[role=combobox]').map(box),buttons:all('button').map(box),tableRows:all('tr').map(box),toolRows:all('[data-tool],.tool-row').map(box),headings:all('h1,h2,h3').map(box),menus:all('[role=menu]').map(n=>({rect:box(n),items:all('[role=menuitem]',n).map(box)})),overflow:document.documentElement.scrollWidth>innerWidth};
 });
}
