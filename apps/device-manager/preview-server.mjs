import http from 'node:http';import fs from 'node:fs/promises';import path from 'node:path';import {fileURLToPath} from 'node:url';
const root=path.join(path.dirname(fileURLToPath(import.meta.url)),'frontend');
const allowed=new Set(['index.html','style.css','app.mjs','design-preview.mjs']);
const server=http.createServer(async(req,res)=>{const name=req.url==='/'?'index.html':req.url?.slice(1);if(req.method!=='GET'||!allowed.has(name)){res.writeHead(404);res.end();return;}try{const data=await fs.readFile(path.join(root,name));res.writeHead(200,{'Content-Type':name.endsWith('.html')?'text/html; charset=utf-8':name.endsWith('.css')?'text/css':'text/javascript','Cache-Control':'no-store','Content-Security-Policy':"default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'"});res.end(data);}catch{res.writeHead(500);res.end();}});
server.listen(4173,'127.0.0.1',()=>console.log('Design preview: http://127.0.0.1:4173 (Ctrl+C to stop; no hardware)'));
