import { useEffect, useState } from 'react';
import { AudioLines, Clapperboard, Download, FileVideo, Image, Scissors, Settings2, Sparkles, Timer, Upload, WandSparkles } from 'lucide-react';
import { getAppInfo, type AppInfo } from './lib/tauri';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';

const tools = [
  { icon: FileVideo, title: 'Medya dönüştür', detail: 'Sıkıştır, dönüştür veya sesi ayır', tag: 'FFmpeg' },
  { icon: Download, title: 'Video indir', detail: 'Bağlantıdan kalite seçerek indir', tag: 'yt-dlp' },
  { icon: AudioLines, title: 'Transkript çıkar', detail: 'Ses ve videoyu metne çevir', tag: 'Yakında' },
  { icon: Image, title: 'Görsel araçları', detail: 'Boyutlandır, sıkıştır, filigran ekle', tag: 'Yakında' },
  { icon: WandSparkles, title: 'Hook fikirleri', detail: 'İlk cümle için yeni fikirler üret', tag: 'Yakında' },
  { icon: Timer, title: 'Sayaçlar', detail: 'Çekim ve kurgu oturumlarını takip et', tag: 'Yakında' },
];
export default function App() {
  const [info, setInfo] = useState<AppInfo>({ name: 'Tamarillo', copyright: 'Copyright EgeHS Medya 2026 - 2027' });
  const [url, setUrl] = useState('');
  const [files, setFiles] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [dragging, setDragging] = useState(false);
  useEffect(() => {
    getAppInfo().then(setInfo).catch(() => undefined);
    let unlisten: (() => void) | undefined;
    getCurrentWebview().onDragDropEvent(event => {
      if (event.payload.type === 'over' || event.payload.type === 'enter') setDragging(true);
      else if (event.payload.type === 'leave') setDragging(false);
      else if (event.payload.type === 'drop') { setDragging(false); setFiles(event.payload.paths); }
    }).then(fn => { unlisten = fn; });
    return () => unlisten?.();
  }, []);
  const addFiles = (paths: string[]) => setFiles(paths);
  const chooseFiles = async () => { const selected = await open({ multiple: true, filters: [{ name: 'Medya', extensions: ['mp4','mkv','mov','webm','avi','mp3','wav','m4a','aac'] }] }); if (selected) addFiles(Array.isArray(selected) ? selected : [selected]); };
  const convert = async (mode: string) => { if (!files[0]) return; setBusy(true); setMessage('Dönüştürme sürüyor…'); try { const result = await invoke<{output:string;message:string}>('convert_media', { input: files[0], mode }); setMessage(`${result.message}: ${result.output}`); } catch (error) { setMessage(String(error)); } finally { setBusy(false); } };
  const download = async () => { if (!url.trim()) return; setBusy(true); setMessage('Video indiriliyor…'); try { const result = await invoke<{output:string;message:string}>('download_video', { url }); setMessage(`${result.message} — ${result.output}`); } catch (error) { setMessage(String(error)); } finally { setBusy(false); } };
  return <div className="shell">
    <aside className="sidebar"><div className="brand"><div className="brand-mark">t</div><div><b>Tamarillo</b><small>CREATOR TOOLKIT</small></div></div>
      <div className="side-label">ÇALIŞMA ALANI</div><button className="nav active"><Clapperboard size={17}/> Genel Bakış</button><button className="nav"><FileVideo size={17}/> Medya araçları</button><button className="nav"><Download size={17}/> İndirmeler</button><button className="nav"><Sparkles size={17}/> Üretim araçları</button>
      <div className="sidebar-bottom"><button className="nav"><Settings2 size={17}/> Ayarlar</button><div className="copyright">{info.copyright}</div></div>
    </aside>
    <main className="main"><header className="topbar"><div className="crumb">Çalışma alanı <span>/</span> Genel Bakış</div><div className="status"><i/> Yerel çalışma alanı</div></header>
      <section className="hero"><div className="eyebrow"><Sparkles size={14}/> ÜRETİMİN İÇİN TEK NOKTA</div><h1>Fikirden yayına,<br/><em>işin daha akıcı.</em></h1><p>Medya dosyalarını dönüştür, içerik üretim araçlarına hızlıca ulaş.</p></section>
      <section className="quick"><div className="section-head"><div><h2>Hızlı işlem</h2><p>Dosya ekle veya bir video bağlantısı yapıştır</p></div></div>
        <div className={`drop-card ${dragging ? 'dragging' : ''}`}><div className="upload-icon"><Upload size={20}/></div><div className="drop-copy"><b>{files.length ? `${files.length} dosya seçildi` : 'Dosyaları buraya bırak'}</b><span>{files.length ? files.map(path => path.split(/[\\/]/).pop()).slice(0, 3).join(', ') : 'Video veya ses dosyası seçerek dönüştürmeye başla'}</span></div><button className="browse" onClick={()=>void chooseFiles()}>Dosya seç</button></div>
        <div className="url-row"><div className="url-icon"><Download size={17}/></div><input value={url} onChange={e=>setUrl(e.target.value)} placeholder="YouTube veya desteklenen video bağlantısını yapıştır"/><button className="primary" disabled={!url.trim() || busy} onClick={()=>void download()}><Download size={15}/> Video indir</button></div>
        {files.length > 0 && <div className="conversion-actions"><span>İlk seçili dosya için:</span><button disabled={busy} onClick={()=>void convert('compress')}>MP4 sıkıştır</button><button disabled={busy} onClick={()=>void convert('mp3')}>MP3 ses çıkar</button><button disabled={busy} onClick={()=>void convert('wav')}>WAV ses çıkar</button><button disabled={busy} onClick={()=>void convert('mp4')}>MP4 dönüştür</button></div>}
        {message && <div className="job-message">{busy && <span className="spinner"/>}{message}</div>}
      </section>
      <section className="tools"><div className="section-head"><div><h2>Araç kutusu</h2><p>İçerik üretim akışın için tasarlandı</p></div><button className="text-button">Tüm araçlar <span>→</span></button></div><div className="tool-grid">{tools.map(({icon:Icon,title,detail,tag})=><button className="tool-card" key={title}><div className="tool-icon"><Icon size={18}/></div><div className="tool-info"><b>{title}</b><span>{detail}</span></div><small className={tag==='Yakında'?'soon':'ready'}>{tag}</small></button>)}</div></section>
      <footer>{info.copyright}<span>•</span> Windows masaüstü sürümü</footer>
    </main>
  </div>;
}
