import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readText } from '@tauri-apps/plugin-clipboard-manager';
import { AudioLines, BookOpen, Check, ChevronRight, Clapperboard, Clock3, Command, Download, FileAudio, FileImage, FileVideo, FolderOpen, Image as ImageIcon, Keyboard, Link2, LoaderCircle, Mic2, Music2, Search, Settings2, Sparkles, Timer, Upload, WandSparkles, X } from 'lucide-react';
import { getAppInfo, type AppInfo } from './lib/tauri';

type Page = 'home' | 'media' | 'download' | 'transcript' | 'images' | 'notes' | 'timer' | 'library' | 'settings';
type Result = { output: string; message: string; content?: string };
type DropModifierState = { shift: boolean; control: boolean; alt: boolean };
const COPYRIGHT = 'Copyright EgeHS Medya 2026 - 2027';
const nav: { id: Page; label: string; icon: typeof Clapperboard }[] = [
  { id: 'home', label: 'Genel Bakış', icon: Clapperboard }, { id: 'media', label: 'Medya dönüştür', icon: FileVideo },
  { id: 'download', label: 'Video indir', icon: Download }, { id: 'transcript', label: 'Transkript & özet', icon: Mic2 },
  { id: 'images', label: 'Görsel araçları', icon: FileImage }, { id: 'notes', label: 'Senaryo & Hook', icon: BookOpen },
  { id: 'timer', label: 'Sayaçlar', icon: Timer }, { id: 'library', label: 'Son işlemler', icon: FolderOpen },
];
const toolNames: { page: Page; title: string; text: string; icon: typeof FileVideo; shortcut: string }[] = [
  { page: 'media', title: 'Medya dönüştür', text: 'Sıkıştır, biçim değiştir, sesi ayır', icon: FileVideo, shortcut: 'Shift + sürükle' },
  { page: 'download', title: 'Video indir', text: 'Video veya MP3 olarak kaydet', icon: Download, shortcut: 'Ctrl + Shift + D' },
  { page: 'transcript', title: 'Transkript çıkar', text: 'Altyazıyı metne al ve özetle', icon: AudioLines, shortcut: 'Ctrl + Space' },
  { page: 'images', title: 'Görsel araçları', text: 'Boyutlandır, sıkıştır, formatla', icon: ImageIcon, shortcut: 'Ctrl + Space' },
  { page: 'notes', title: 'Senaryo notları', text: 'Not al ve kanca fikirleri üret', icon: WandSparkles, shortcut: 'Ctrl + F' },
  { page: 'timer', title: 'Çekim sayacı', text: 'Odak ve çekim zamanlayıcısı', icon: Clock3, shortcut: 'Ctrl + Space' },
];
const fileFilters = [{ name: 'Medya ve görseller', extensions: ['mp4','mkv','mov','webm','avi','mp3','wav','m4a','aac','flac','ogg','png','jpg','jpeg','webp','bmp','tiff'] }];

function parseVtt(input: string) {
  return input.replace(/^WEBVTT[^\n]*\n?/m, '').split('\n').filter(line => line.trim() && !line.includes('-->') && !/^\d+$/.test(line.trim()) && !line.startsWith('NOTE')).join(' ').replace(/<[^>]+>/g, '').replace(/\s+/g, ' ').trim();
}
function makeSummary(text: string) {
  const sentences = text.split(/(?<=[.!?])\s+/).map(s => s.trim()).filter(s => s.length > 25);
  if (sentences.length < 3) return sentences.join(' ');
  const words = text.toLocaleLowerCase('tr').match(/[\p{L}]{4,}/gu) ?? [];
  const stop = new Set('olarak için ancak fakat çünkü üzere daha sonra kadar veya ile olan bu bir ve da de'.split(' '));
  const freq = new Map<string, number>(); words.forEach(w => { if (!stop.has(w)) freq.set(w, (freq.get(w) ?? 0) + 1); });
  return sentences.map((s, i) => ({ s, i, score: (s.toLocaleLowerCase('tr').match(/[\p{L}]{4,}/gu) ?? []).reduce((n, w) => n + (freq.get(w) ?? 0), 0) / Math.max(1, s.length) }))
    .sort((a,b) => b.score - a.score).slice(0, Math.min(5, Math.ceil(sentences.length * .25))).sort((a,b) => a.i - b.i).map(x => x.s).join(' ');
}

export default function App() {
  const [info, setInfo] = useState<AppInfo>({ name: 'Tamarillo', copyright: COPYRIGHT });
  const [page, setPage] = useState<Page>('home');
  const [files, setFiles] = useState<string[]>([]);
  const [url, setUrl] = useState('');
  const [downloadFormat, setDownloadFormat] = useState<'video'|'mp3'>('video');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [outputs, setOutputs] = useState<string[]>([]);
  const [dragging, setDragging] = useState(false);
  const dragMode = useRef<'tools'|'convert'|'file'|null>(null);
  const [menu, setMenu] = useState<'tools'|'convert'|'file'|null>(null);
  const [search, setSearch] = useState('');
  const [transcript, setTranscript] = useState('');
  const [summary, setSummary] = useState('');
  const [imgAction, setImgAction] = useState<'resize'|'compress'|'convert'|'watermark'>('resize');
  const [imgWidth, setImgWidth] = useState('1920');
  const [imgHeight, setImgHeight] = useState('1080');
  const [imgFormat, setImgFormat] = useState('png');
  const [imgQuality, setImgQuality] = useState('85');
  const [watermark, setWatermark] = useState('');
  const [topic, setTopic] = useState('');
  const [hooks, setHooks] = useState<string[]>([]);
  const [note, setNote] = useState(() => localStorage.getItem('tamarillo-script-notes') ?? '');
  const [seconds, setSeconds] = useState(25 * 60);
  const [timerRunning, setTimerRunning] = useState(false);
  const [timerLabel, setTimerLabel] = useState('Odak oturumu');
  const [timerComplete, setTimerComplete] = useState(false);
  const [library, setLibrary] = useState<string[]>(() => JSON.parse(localStorage.getItem('tamarillo-history') ?? '[]'));
  const searchRef = useRef<HTMLInputElement>(null);
  const heldModifiers = useRef({ shift: false, control: false, alt: false });

  const addFiles = (paths: string[]) => { setFiles(paths); if (paths.length) { setOutputs([]); setMessage(`${paths.length} dosya hazır`); } };
  const chooseFiles = async (kind: 'media'|'images' = 'media') => {
    const extensions = kind === 'images' ? ['png','jpg','jpeg','webp','bmp','tiff','gif','avif','heic'] : fileFilters[0].extensions;
    const selected = await open({ multiple: true, filters: [{ name: kind === 'images' ? 'Görseller' : 'Medya ve görseller', extensions }] });
    if (selected) addFiles(Array.isArray(selected) ? selected : [selected]);
  };
  const remember = (items: string[]) => {
    const updated = [...items, ...library].filter((v, i, a) => a.indexOf(v) === i).slice(0, 30);
    setLibrary(updated); localStorage.setItem('tamarillo-history', JSON.stringify(updated)); setOutputs(items);
  };
  const runMedia = async (mode: string) => {
    if (!files.length) { await chooseFiles(); return; }
    setBusy(true); setMessage('İşlem sürüyor…'); const done: string[] = [];
    try {
      for (const input of files) { setMessage(`${input.split(/[\\/]/).pop()} işleniyor…`); const result = await invoke<Result>('convert_media', { input, mode, quality: 23 }); done.push(result.output); }
      remember(done); setMessage(`${done.length} dosya için işlem tamamlandı`);
    } catch (error) { setMessage(String(error)); } finally { setBusy(false); }
  };
  const trimSilence = async () => {
    if (!files.length) { await chooseFiles(); return; }
    setBusy(true); setMessage('Sessizlik analizi yapılıyor…'); const done: string[] = [];
    try { for (const input of files) { const result = await invoke<Result>('trim_video_silence', { input, threshold: -38, minSilence: 0.45 }); done.push(result.output); } remember(done); setMessage('Sessiz bölümler kesildi'); }
    catch (error) { setMessage(String(error)); } finally { setBusy(false); }
  };
  const runDownload = async (providedUrl?: string) => {
    const link = (providedUrl ?? url).trim(); if (!link) { setMessage('Önce bir bağlantı yapıştır'); return; }
    setUrl(link); setBusy(true); setMessage('İndirme başladı…');
    try { const result = await invoke<Result>('download_video', { url: link, format: downloadFormat }); remember([result.output]); setMessage(`${result.message} · Kayıt yeri: ${result.output}`); }
    catch (error) { setMessage(String(error)); } finally { setBusy(false); }
  };
  const extractTranscript = async () => {
    if (!url.trim()) { setMessage('YouTube bağlantısını gir'); return; }
    setBusy(true); setMessage('Türkçe / İngilizce altyazı aranıyor…');
    try { const result = await invoke<Result>('download_captions', { url, language: 'tr,en' }); if (!result.content) throw new Error('Bu video için altyazı bulunamadı. Varsa altyazı metnini aşağıdaki alana yapıştırabilirsin.'); const text = parseVtt(result.content); setTranscript(text); setSummary(''); remember([result.output]); setMessage(`Altyazı çıkarıldı · ${result.output}`); }
    catch (error) { setMessage(String(error)); } finally { setBusy(false); }
  };
  const transcribeAudio = async () => {
    let input = files[0];
    if (!input) { const selected = await open({ multiple: false, filters: [{ name: 'Ses veya video', extensions: ['mp4','mkv','mov','webm','avi','mp3','wav','m4a','aac','flac','ogg'] }] }); if (typeof selected !== 'string') return; input = selected; setFiles([selected]); }
    setBusy(true); setMessage('Yerel konuşma tanıma hazırlanıyor… İlk kullanımda yaklaşık 142 MB model indirilir.');
    try { const result = await invoke<Result>('transcribe_media', { input }); if (!result.content) throw new Error('Transkript boş döndü'); setTranscript(result.content); setSummary(''); remember([result.output]); setMessage(`Transkript hazır · ${result.output}`); }
    catch (error) { setMessage(String(error)); } finally { setBusy(false); }
  };
  const transcribeLink = async () => {
    if (!url.trim()) { setMessage('YouTube bağlantısını gir'); return; }
    setBusy(true); setMessage('Ses akışı indiriliyor ve yerel modelle çözümleniyor… İlk kullanımda yaklaşık 142 MB model indirilir.');
    try { const result = await invoke<Result>('transcribe_url', { url }); if (!result.content) throw new Error('Transkript boş döndü'); setTranscript(result.content); setSummary(''); remember([result.output]); setMessage(`Transkript hazır · ${result.output}`); }
    catch (error) { setMessage(String(error)); } finally { setBusy(false); }
  };
  const processImages = async () => {
    if (!files.length) { await chooseFiles('images'); return; }
    setBusy(true); setMessage('Görseller işleniyor…');
    try { const results = await invoke<Result[]>('process_images', { inputs: files, action: imgAction, width: Number(imgWidth), height: Number(imgHeight), format: imgFormat, quality: Number(imgQuality), watermark: watermark || null }); const paths = results.map(r => r.output); remember(paths); setMessage(`${paths.length} görsel hazırlandı`); }
    catch (error) { setMessage(String(error)); } finally { setBusy(false); }
  };
  const generateHooks = () => {
    const subject = topic.trim() || 'bu konu';
    setHooks([`${subject} hakkında çoğu kişinin yanlış bildiği şey şu…`, `Bu ${subject} hatası sana düşündüğünden daha pahalıya patlıyor.`, `${subject} için keşke daha önce bilseydim dediğim 3 şey.`, `30 saniyede ${subject} konusundaki en önemli noktayı anlatıyorum.`, `Herkes ${subject} yapıyor ama şu detayı atlıyor.`]);
  };

  useEffect(() => { getAppInfo().then(setInfo).catch(() => undefined); let stop: (()=>void)|undefined; getCurrentWebview().onDragDropEvent(async event => {
    const payload = event.payload;
    if (payload.type === 'enter' || payload.type === 'over') {
      setDragging(true);
    } else if (payload.type === 'leave') {
      setDragging(false);
    } else if (payload.type === 'drop') {
      const paths = payload.paths;
      setDragging(false);
      if (paths.length) {
        addFiles(paths);
        try {
          const keys = await invoke<DropModifierState>('drop_modifiers');
          if (keys.shift) dragMode.current = 'convert';
          else if (keys.alt) dragMode.current = 'tools';
          else if (keys.control) dragMode.current = 'file';
        } catch { /* Keep the mode observed while the drag was over the app. */ }
        const active = dragMode.current;
        if (active) {
          setMenu(active);
          // If the user released over a wheel petal, open its matching tool.
          const scale = window.devicePixelRatio || 1;
          const x = payload.position.x / scale, y = payload.position.y / scale;
          const target = Array.from(document.querySelectorAll<HTMLButtonElement>('.wheel-grid button')).find(button => {
            const rect = button.getBoundingClientRect(); return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
          });
          if (target) {
            const label = target.querySelector('b')?.textContent;
            const item = toolNames.find(tool => tool.title === label);
            if (item) { setPage(item.page); setMenu(null); }
          }
        }
      }
      dragMode.current = null;
    }
  }).then(fn => { stop = fn; }); return () => stop?.(); }, []);
  useEffect(() => {
    if (!dragging) return;
    // Poll while a file drag is in progress: Windows does not send key events
    // to the WebView because Explorer owns the mouse capture.
    const id = window.setInterval(() => {
      void invoke<DropModifierState>('drop_modifiers').then(keys => {
        const next = keys.shift ? 'convert' : keys.alt ? 'tools' : keys.control ? 'file' : null;
        if (next) { dragMode.current = next; setMenu(next); }
      }).catch(() => undefined);
    }, 40);
    return () => window.clearInterval(id);
  }, [dragging]);
  useEffect(() => { localStorage.setItem('tamarillo-script-notes', note); }, [note]);
  useEffect(() => { if (!timerRunning) return; const id = window.setInterval(() => setSeconds(v => { if (v <= 1) { setTimerRunning(false); setTimerComplete(true); return 0; } return v - 1; }), 1000); return () => clearInterval(id); }, [timerRunning]);
  useEffect(() => {
    let unlisten: (()=>void)|undefined; listen('shortcut-download-link', async () => { try { const copied = await readText(); if (/^https?:\/\//i.test(copied.trim())) { setUrl(copied.trim()); setPage('download'); setMessage('Panodaki bağlantı hazır'); } else { setPage('download'); setMessage('Panoda bağlantı bulunamadı; bağlantıyı yapıştır.'); } } catch { setPage('download'); setMessage('Panodan okunamadı; bağlantıyı yapıştır.'); } }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);
  useEffect(() => {
    let unlisten: (()=>void)|undefined; let unlistenResult: (()=>void)|undefined;
    listen<{paths:string[];page:string|null}>('desktop-file-dropped', event => {
      setFiles(event.payload.paths); setOutputs([]); setMessage(`${event.payload.paths.length} dosya hazır`);
      const target = nav.find(item => item.id === event.payload.page);
      setPage(target?.id ?? 'home'); setMenu(null);
    }).then(fn => { unlisten = fn; });
    listen<{ok:boolean;running?:boolean;message:string;outputs:string[]}>('desktop-action-result', event => {
      setBusy(Boolean(event.payload.running)); setMessage(event.payload.message); setOutputs(event.payload.outputs ?? []);
      if (event.payload.outputs?.length) setLibrary(current => { const next = [...event.payload.outputs, ...current].filter((value,index,array)=>array.indexOf(value)===index).slice(0,30); localStorage.setItem('tamarillo-history',JSON.stringify(next)); return next; });
    }).then(fn => { unlistenResult = fn; });
    return () => { unlisten?.(); unlistenResult?.(); };
  }, []);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      heldModifiers.current = { shift: event.shiftKey, control: event.ctrlKey, alt: event.altKey };
      const target = event.target as HTMLElement | null; const typing = target && (['INPUT','TEXTAREA','SELECT'].includes(target.tagName) || target.isContentEditable);
      if ((event.ctrlKey || event.metaKey) && event.code === 'Space') { event.preventDefault(); setMenu('tools'); }
      else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'o') { event.preventDefault(); void chooseFiles(); }
      else if ((event.ctrlKey || event.metaKey) && event.key === '2' && !typing) { event.preventDefault(); setPage('library'); }
      else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') { event.preventDefault(); setMenu('tools'); window.setTimeout(() => searchRef.current?.focus(), 20); }
      else if (event.key === 'Escape' || (menu && event.key === 'Tab')) { if (menu) { event.preventDefault(); setMenu(null); } }
      else if (menu && ['ArrowDown','ArrowRight','ArrowUp','ArrowLeft','Home','End'].includes(event.key)) { event.preventDefault(); const buttons=Array.from(document.querySelectorAll<HTMLButtonElement>('.wheel-grid button')); if(event.key==='Home') buttons[0]?.focus(); else if(event.key==='End') buttons[buttons.length-1]?.focus(); else { const active=buttons.indexOf(document.activeElement as HTMLButtonElement); const delta=event.key==='ArrowDown'||event.key==='ArrowRight'?1:-1; buttons[active<0?0:(active+delta+buttons.length)%buttons.length]?.focus(); } }
    };
    const keyup=()=>{heldModifiers.current={shift:false,control:false,alt:false};};
    window.addEventListener('keydown', key); window.addEventListener('keyup', keyup); return () => {window.removeEventListener('keydown', key);window.removeEventListener('keyup',keyup);};
  }, [menu]);

  const filteredTools = useMemo(() => toolNames.filter(t => `${t.title} ${t.text}`.toLocaleLowerCase('tr').includes(search.toLocaleLowerCase('tr'))), [search]);
  const title = nav.find(n => n.id === page)?.label ?? 'Genel Bakış';
  const openPage = (next: Page) => { setPage(next); setMenu(null); setMessage(''); };
  const initials = info.name?.slice(0, 1).toLocaleLowerCase('tr') || 't';
  const formatTime = `${String(Math.floor(seconds / 60)).padStart(2,'0')}:${String(seconds % 60).padStart(2,'0')}`;
  const wheelItems = menu==='tools' ? filteredTools : toolNames.filter(t=>menu==='convert'?t.page==='media'||t.page==='images':t.page==='library'||t.page==='media'||t.page==='download');

  return <div className="shell">
    <aside className="sidebar"><div className="brand"><div className="brand-mark">{initials}</div><div><b>Tamarillo</b><small>CREATOR TOOLKIT</small></div></div><div className="side-label">ÇALIŞMA ALANI</div>
      {nav.map(item => { const Icon = item.icon; return <button key={item.id} className={`nav ${page===item.id?'active':''}`} onClick={()=>openPage(item.id)}><Icon size={17}/>{item.label}</button>; })}
      <div className="sidebar-bottom"><button className={`nav ${page==='settings'?'active':''}`} onClick={()=>openPage('settings')}><Settings2 size={17}/>Ayarlar</button><div className="copyright">{info.copyright}</div></div>
    </aside>
    <main className="main"><header className="topbar"><div className="crumb">Çalışma alanı <span>/</span> {title}</div><div className="status"><i/> Yerel çalışma alanı</div></header>
      {page==='home' && <>
        <section className="hero"><div className="eyebrow"><Sparkles size={14}/> ÜRETİMİN İÇİN TEK NOKTA</div><h1>Fikirden yayına,<br/><em>işin daha akıcı.</em></h1><p>Dosyayı bırak, aracını seç, işine devam et. Dosyaların bilgisayarında işlenir.</p></section>
        <section className="quick"><div className="section-head"><div><h2>Hızlı işlem</h2><p>Dosya ekle veya bir video bağlantısı yapıştır</p></div><button className="key-hint" onClick={()=>setMenu('tools')}><Command size={13}/> Ctrl + Space</button></div>
          <div className={`drop-card ${dragging?'dragging':''}`} onClick={()=>!files.length&&void chooseFiles()}><div className="upload-icon"><Upload size={20}/></div><div className="drop-copy"><b>{files.length?`${files.length} dosya seçildi`:'Dosyaları buraya bırak'}</b><span>{files.length?files.map(path=>path.split(/[\\/]/).pop()).slice(0,3).join(', '):'Shift: dönüştür · Ctrl: araçlar · Alt: dosya işlemleri'}</span></div><button className="browse" onClick={e=>{e.stopPropagation();void chooseFiles();}}>Dosya seç</button></div>
          <div className="url-row"><div className="url-icon"><Link2 size={17}/></div><input value={url} onChange={e=>setUrl(e.target.value)} onKeyDown={e=>e.key==='Enter'&&void runDownload()} placeholder="YouTube veya desteklenen video bağlantısını yapıştır"/><select value={downloadFormat} onChange={e=>setDownloadFormat(e.target.value as 'video'|'mp3')}><option value="video">En iyi video</option><option value="mp3">MP3 ses</option></select><button className="primary" disabled={!url.trim()||busy} onClick={()=>void runDownload()}><Download size={15}/> İndir</button></div>
          {files.length>0&&<div className="conversion-actions"><span>Hızlı dönüştür:</span><button disabled={busy} onClick={()=>void runMedia('compress')}>MP4 sıkıştır</button><button disabled={busy} onClick={()=>void runMedia('mp3')}>MP3 çıkar</button><button disabled={busy} onClick={()=>void runMedia('wav')}>WAV çıkar</button><button disabled={busy} onClick={()=>void runMedia('mp4')}>MP4 dönüştür</button></div>}
          {message&&<Status busy={busy} message={message} outputs={outputs}/>}</section>
        <section className="tools"><div className="section-head"><div><h2>Araç kutusu</h2><p>İçerik üretim akışın için tasarlandı</p></div><button className="text-button" onClick={()=>setMenu('tools')}>Tüm araçlar <span>→</span></button></div><div className="tool-grid">{toolNames.map(({page:p,title:label,text,icon:Icon,shortcut})=><button className="tool-card" key={label} onClick={()=>openPage(p)}><div className="tool-icon"><Icon size={18}/></div><div className="tool-info"><b>{label}</b><span>{text}</span></div><small className="ready">{shortcut}</small></button>)}</div></section>
      </>}

      {page==='media'&&<><PageIntro eyebrow="FFMPEG · YEREL İŞLEME" title="Medya araçları" detail="Dosyalarını dönüştür, sıkıştır veya sesi videodan ayır."/><FileDrop files={files} dragging={dragging} onChoose={()=>void chooseFiles()} onClear={()=>setFiles([])}/><div className="panel"><h2>Bir işlem seç</h2><p className="muted">Birden fazla dosya seçtiysen işlem hepsine sırayla uygulanır. Çıktılar kaynak dosyanın yanına yeni kopya olarak yazılır.</p><div className="action-grid">{[['compress','MP4 sıkıştır','Dosya boyutunu küçült','FileVideo'],['mp4','MP4 dönüştür','H.264 + AAC','Clapperboard'],['mov','MOV oluştur','H.264 + AAC','FileVideo'],['webm','WebM oluştur','VP9 + Opus','FileVideo'],['mp3','MP3 ses ayır','Videodan sesi çıkar','Music2'],['wav','WAV ses ayır','Kayıpsız PCM ses','FileAudio'],['silence-audio','Sessizliği kırp (ses)','Sessiz aralıkları kaldırıp MP3 üret','AudioLines']].map(([mode,label,desc])=><button className="action-card" key={mode} disabled={busy} onClick={()=>void runMedia(mode)}><b>{label}</b><span>{desc}</span><ChevronRight size={16}/></button>)}</div><div className="split-row"><div><h3>Videodaki sessiz bölümleri kes</h3><p className="muted">Ses seviyesini analiz eder, sessiz aralıkları çıkarıp yeni MP4 üretir.</p></div><button className="primary" disabled={busy||!files.length} onClick={()=>void trimSilence()}>Sessizliği kes</button></div>{message&&<Status busy={busy} message={message} outputs={outputs}/>}</div></>}

      {page==='download'&&<><PageIntro eyebrow="YT-DLP · İNDİRME KLASÖRÜ" title="Video veya ses indir" detail="Desteklenen herkese açık bağlantıları en iyi mevcut kalitede kaydet."/><div className="panel"><label className="field-label">Video bağlantısı</label><div className="url-row"><div className="url-icon"><Link2 size={17}/></div><input value={url} onChange={e=>setUrl(e.target.value)} onKeyDown={e=>e.key==='Enter'&&void runDownload()} placeholder="https://…"/><button className="browse" onClick={async()=>{try{const s=await readText();if(/^https?:\/\//i.test(s.trim()))setUrl(s.trim());}catch{setMessage('Panoya erişilemedi');}}}>Panodan yapıştır</button></div><div className="form-row"><label><input type="radio" checked={downloadFormat==='video'} onChange={()=>setDownloadFormat('video')}/> En yüksek video kalitesi</label><label><input type="radio" checked={downloadFormat==='mp3'} onChange={()=>setDownloadFormat('mp3')}/> MP3 ses</label></div><button className="primary" disabled={!url.trim()||busy} onClick={()=>void runDownload()}><Download size={15}/> İndirmeyi başlat</button><p className="muted small">Dosyalar Windows İndirilenler klasörüne kaydedilir. Giriş isteyen, DRM korumalı ve canlı yayınlar desteklenmez.</p>{message&&<Status busy={busy} message={message} outputs={outputs}/>}</div></>}

      {page==='transcript'&&<><PageIntro eyebrow="WHISPER · YEREL ÖZET" title="Transkript çıkar & özetle" detail="YouTube altyazısından veya ses/video dosyasından konuşmayı metne çevir; özetini cihazında oluştur."/><div className="panel"><label className="field-label">YouTube bağlantısı</label><div className="url-row"><div className="url-icon"><Link2 size={17}/></div><input value={url} onChange={e=>setUrl(e.target.value)} placeholder="https://youtube.com/watch?v=…"/><button className="browse" disabled={busy} onClick={()=>void extractTranscript()}><Mic2 size={15}/> Altyazıyı al</button></div><div className="button-row"><button className="browse" disabled={busy} onClick={()=>void transcribeLink()}><Mic2 size={15}/> Bağlantıdaki konuşmayı tanı</button><button className="browse" disabled={busy} onClick={()=>void transcribeAudio()}><FileAudio size={15}/> Ses/video dosyasını metne çevir</button></div><p className="muted small">Whisper modeli ilk kullanımda yaklaşık 142 MB olarak indirilir. Sonraki transkriptler çevrimdışı ve yerel çalışır. Altyazı indirme, video altyazı sağlıyorsa kullanılabilir.</p><label className="field-label">Transkript metni</label><textarea className="transcript-box" value={transcript} onChange={e=>{setTranscript(e.target.value);setSummary('');}} placeholder="Altyazı veya transkript burada görünür; istersen metni yapıştır…"/><div className="button-row"><button className="primary" disabled={!transcript.trim()} onClick={()=>setSummary(makeSummary(transcript))}><Sparkles size={15}/> Yerel özet oluştur</button><span className="muted small">Metin cihazdan gönderilmez.</span></div>{summary&&<div className="summary-card"><b>Özet</b><p>{summary}</p><button className="browse" onClick={()=>void navigator.clipboard.writeText(summary)}>Özeti kopyala</button></div>}{message&&<Status busy={busy} message={message} outputs={outputs}/>}</div></>}

      {page==='images'&&<><PageIntro eyebrow="TOPLU · YEREL İŞLEME" title="Görsel araçları" detail="Birden fazla görseli yeniden boyutlandır, sıkıştır, çevir veya filigranla."/><FileDrop files={files} dragging={dragging} onChoose={()=>void chooseFiles('images')} onClear={()=>setFiles([])}/><div className="panel"><label className="field-label">İşlem türü</label><div className="segmented">{(['resize','compress','convert','watermark'] as const).map((action,i)=><button key={action} className={imgAction===action?'selected':''} onClick={()=>setImgAction(action)}>{['Boyutlandır','Sıkıştır','Format çevir','Filigran'][i]}</button>)}</div>{imgAction==='resize'&&<div className="form-row"><label>Genişlik <input className="number-input" type="number" value={imgWidth} onChange={e=>setImgWidth(e.target.value)}/></label><label>Yükseklik <input className="number-input" type="number" value={imgHeight} onChange={e=>setImgHeight(e.target.value)}/></label></div>}{imgAction==='convert'&&<div className="form-row"><label>Çıktı formatı <select value={imgFormat} onChange={e=>setImgFormat(e.target.value)}><option>png</option><option>jpg</option><option>webp</option></select></label></div>}{imgAction==='compress'&&<div className="form-row"><label>JPEG kalite: {imgQuality}% <input type="range" min="10" max="100" value={imgQuality} onChange={e=>setImgQuality(e.target.value)}/></label></div>}{imgAction==='watermark'&&<div className="form-row"><button className="browse" onClick={async()=>{const f=await open({multiple:false,filters:[{name:'Filigran',extensions:['png','jpg','webp']} ]});if(typeof f==='string')setWatermark(f);}}>{watermark?watermark.split(/[\\/]/).pop():'Filigran dosyası seç'}</button></div>}<button className="primary" disabled={busy||!files.length} onClick={()=>void processImages()}><WandSparkles size={15}/> {files.length||'Görselleri'} işle</button>{message&&<Status busy={busy} message={message} outputs={outputs}/>}</div></>}

      {page==='notes'&&<><PageIntro eyebrow="YEREL NOT DEFTERİ" title="Senaryo & Hook" detail="Notların otomatik olarak bu bilgisayarda saklanır."/><div className="two-panels"><div className="panel"><div className="section-head"><div><h2>Senaryo notları</h2><p>Otomatik kaydedilir</p></div><Check size={16} className="ok-icon"/></div><textarea className="notes-box" value={note} onChange={e=>setNote(e.target.value)} placeholder="Video fikrini, sahne akışını veya çekim notlarını yaz…"/></div><div className="panel"><h2>Hook fikir üretici</h2><p className="muted">Konu başlığını yaz; beş yerel şablon üretelim.</p><input className="plain-input" value={topic} onChange={e=>setTopic(e.target.value)} placeholder="Örn. YouTube'da ilk 1000 abone"/><button className="primary" onClick={generateHooks}><Sparkles size={15}/> Hook üret</button><div className="hook-list">{hooks.map((hook,i)=><div className="hook-item" key={hook}><span>0{i+1}</span><p>{hook}</p><button className="icon-button" title="Kopyala" onClick={()=>void navigator.clipboard.writeText(hook)}>Kopyala</button></div>)}</div></div></div></>}

      {page==='timer'&&<><PageIntro eyebrow="ODAK · ÇEKİM · KURGU" title="Sayaçlar" detail="Pomodoro veya çekim süresi için basit sayaç."/><div className="timer-panel"><input className="timer-label" value={timerLabel} onChange={e=>setTimerLabel(e.target.value)}/><div className="timer-face">{formatTime}</div><div className="button-row"><button className="primary" onClick={()=>{setTimerComplete(false);setTimerRunning(!timerRunning);}}>{timerRunning?'Duraklat':'Başlat'}</button><button className="browse" onClick={()=>{setTimerRunning(false);setTimerComplete(false);setSeconds(25*60);}}>Sıfırla</button><button className="browse" onClick={()=>{setTimerRunning(false);setTimerComplete(false);setSeconds(5*60);}}>5 dk mola</button><button className="browse" onClick={()=>{setTimerRunning(false);setTimerComplete(false);setSeconds(25*60);}}>25 dk odak</button></div>{timerComplete&&<div className="timer-done">{timerLabel} tamamlandı</div>}<p className="muted small">Sayaç tamamlandığında bu ekranda bildirim gösterilir.</p></div></>}

      {page==='library'&&<><PageIntro eyebrow="BU CİHAZDA" title="Son işlemler" detail="Son çıktılarının klasör yolları bu bilgisayarda saklanır."/>{library.length===0?<div className="empty-state">Henüz tamamlanan bir işlem yok.</div>:<div className="panel history-list">{library.map((path,i)=><div className="history-item" key={`${path}-${i}`}><FileVideo size={17}/><span>{path}</span><button className="icon-button" onClick={()=>void navigator.clipboard.writeText(path)}>Yolu kopyala</button></div>)}</div>}</>}

      {page==='settings'&&<><PageIntro eyebrow="TERCİHLER" title="Ayarlar" detail="Tamarillo’nun Windows kısayolları ve çalışma şekli."/><div className="panel"><h2>Windows kısayolları</h2><ShortcutTable/><div className="split-row"><div><h3>Genel kısayol</h3><p className="muted">Uygulama arka plandayken panodaki bağlantıyı Tamarillo’ya getirir.</p></div><kbd>Ctrl + Shift + D</kbd></div><p className="muted small">Explorer’da dosyayı tutup masaüstünde sürüklerken Shift dönüştürme, Alt araç, Ctrl dosya işlemleri çarkını açar. Çarktaki seçeneğin üzerine bırak.</p><div className="setting-row"><span>Görünüm</span><span className="pill">Koyu tema</span></div><div className="setting-row"><span>Uygulama</span><b>{info.name}</b></div><div className="setting-row"><span>Telif</span><b>{info.copyright}</b></div></div></>}

      <footer>{info.copyright}<span>•</span> Windows masaüstü sürümü</footer>
    </main>
    {menu&&<div className="modal-backdrop" onMouseDown={e=>e.target===e.currentTarget&&setMenu(null)}><div className="wheel-modal"><div className="modal-head"><div><small>HIZLI ERİŞİM · WINDOWS</small><h2>{menu==='tools'?'Bir araç seç':menu==='convert'?'Dönüştürme çarkı':'Dosya işlemi seç'}</h2></div><button className="icon-button" onClick={()=>setMenu(null)}><X size={18}/></button></div>{menu==='tools'&&<div className="search-box"><Search size={16}/><input ref={searchRef} value={search} onChange={e=>setSearch(e.target.value)} placeholder="Araçlarda ara…"/><kbd>Ctrl F</kbd></div>}<div className="wheel-grid"><div className="wheel-core"><span>t</span><small>TAMARILLO</small></div>{wheelItems.map(({page:p,title:label,text,icon:Icon,shortcut},index)=>{const angle=(360*index/Math.max(wheelItems.length,1))-90;return <button key={label} style={{'--angle':`${angle}deg`,'--reverse-angle':`${-angle}deg`} as CSSProperties} onClick={()=>openPage(p)}><span className="tool-icon"><Icon size={18}/></span><span><b>{label}</b><small>{text}</small></span><kbd>{shortcut}</kbd></button>;})}</div><div className="modal-foot">↑ ↓ seç · Enter aç · Esc kapat <span>Ctrl + O dosya seç</span></div></div></div>}
  </div>;
}

function Status({busy,message,outputs}:{busy:boolean;message:string;outputs:string[]}) { const failed=!busy&&/çalıştırılamadı|açılamadı|başarısız|bulunamadı|hata verdi|boş döndü|okunamadı|indirilemedi/i.test(message); return <div className={`job-message ${failed?'failed':''}`}>{busy?<LoaderCircle size={14} className="spin"/>:failed?<X size={14}/>:<Check size={14}/>}<span>{message}</span>{outputs.length>0&&<div className="output-list">{outputs.slice(0,8).map(path=><code key={path}>{path}</code>)}</div>}</div>; }
function PageIntro({eyebrow,title,detail}:{eyebrow:string;title:string;detail:string}) { return <section className="page-intro"><div className="eyebrow">{eyebrow}</div><h1>{title}</h1><p>{detail}</p></section>; }
function FileDrop({files,dragging,onChoose,onClear}:{files:string[];dragging:boolean;onChoose:()=>void;onClear:()=>void}) { return <div className={`drop-card file-drop ${dragging?'dragging':''}`}><div className="upload-icon"><Upload size={20}/></div><div className="drop-copy"><b>{files.length?`${files.length} dosya seçildi`:'Dosyaları buraya bırak'}</b><span>{files.length?files.map(f=>f.split(/[\\/]/).pop()).slice(0,4).join(', '):'Birden fazla dosya seçebilirsin'}</span></div><button className="browse" onClick={onChoose}>Dosya seç</button>{files.length>0&&<button className="icon-button" onClick={onClear}><X size={16}/></button>}</div>; }
function ShortcutTable() { const rows=[['Shift + sürükleme','Dönüştürme çarkı'],['Alt + sürükleme','Araç çarkı'],['Ctrl + sürükleme','Dosya işlemleri çarkı'],['Ctrl + Shift + D','Panodaki bağlantıyı getir (genel kısayol)'],['Ctrl + O','Dosya seç'],['Ctrl + Space','Araç çarkını aç'],['Ctrl + 2','Son işlemleri aç'],['Ctrl + F','Araçları ara'],['↑ / ↓ / ← / →','Çarkta araç seç'],['Home / End','İlk / son araca git'],['Enter / Space','Seçili aracı aç'],['Esc / Tab','Çarkı kapat']];return <div className="shortcut-table">{rows.map(([key,action])=><div key={key+action}><kbd>{key}</kbd><span>{action}</span></div>)}</div>; }
