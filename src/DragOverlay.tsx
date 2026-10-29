import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';

type Mode = 'convert' | 'tools' | 'file';
type Keys = { shift: boolean; control: boolean; alt: boolean };
type Item = { page: string; title: string; detail: string };
const items: Record<Mode, Item[]> = {
  convert: [
    { page: 'media', title: 'Medya dönüştür', detail: 'Video ve ses' },
    { page: 'images', title: 'Görsel araçları', detail: 'Boyutlandır ve biçimle' },
  ],
  tools: [
    { page: 'transcript', title: 'Transkript çıkar', detail: 'Video veya ses' },
    { page: 'media', title: 'Medya dönüştür', detail: 'Sıkıştır ve ses ayır' },
    { page: 'images', title: 'Görsel araçları', detail: 'Düzenle ve dönüştür' },
    { page: 'download', title: 'Video indir', detail: 'YouTube bağlantısı' },
    { page: 'notes', title: 'Senaryo notları', detail: 'Notlar ve hook fikirleri' },
    { page: 'timer', title: 'Çekim sayacı', detail: 'Zamanlayıcılar' },
  ],
  file: [
    { page: 'library', title: 'Son işlemler', detail: 'Çıktı geçmişi' },
    { page: 'media', title: 'Medya dönüştür', detail: 'Dosyayı işle' },
    { page: 'download', title: 'Video indir', detail: 'Bağlantıdan indir' },
  ],
};

function modeFrom(keys: Keys): Mode | null {
  return keys.shift ? 'convert' : keys.alt ? 'tools' : keys.control ? 'file' : null;
}

export default function DragOverlay() {
  const [active, setActive] = useState(false);
  const [mode, setMode] = useState<Mode | null>(null);
  const [paths, setPaths] = useState<string[]>([]);
  const [anchor, setAnchor] = useState({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
  const [cursor, setCursor] = useState({ x: -1, y: -1 });
  const [hover, setHover] = useState(-1);
  const receivedFiles = useRef(false);
  const pathsRef = useRef<string[]>([]);

  useEffect(() => {
    document.documentElement.style.background = 'transparent';
    document.body.style.background = 'transparent';
    let stopDrop: (() => void) | undefined;
    let stopSession: (() => void) | undefined;
    getCurrentWebview().onDragDropEvent(event => {
      const scale = window.devicePixelRatio || 1;
      const p = event.payload;
      if (p.type === 'enter') {
        if (!p.paths.length) return;
        receivedFiles.current = true;
        pathsRef.current = p.paths;
        setPaths(p.paths);
        setActive(true);
        setAnchor({ x: p.position.x / scale, y: p.position.y / scale });
        setCursor({ x: p.position.x / scale, y: p.position.y / scale });
      } else if (p.type === 'over') {
        const point = { x: p.position.x / scale, y: p.position.y / scale };
        setCursor(point);
        const button = Array.from(document.querySelectorAll<HTMLElement>('.global-drag-wheel [data-page]')).find(node => {
          const rect = node.getBoundingClientRect();
          return point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom;
        });
        setHover(button ? Array.from(document.querySelectorAll('.global-drag-wheel [data-page]')).indexOf(button) : -1);
      } else if (p.type === 'drop') {
        const dropped = p.paths.length ? p.paths : pathsRef.current;
        const scaleNow = window.devicePixelRatio || 1;
        const x = p.position.x / scaleNow, y = p.position.y / scaleNow;
        const buttons = Array.from(document.querySelectorAll<HTMLElement>('.global-drag-wheel [data-page]'));
        const target = buttons.find(node => {
          const rect = node.getBoundingClientRect();
          return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
        });
        const page = target?.dataset.page ?? null;
        if (dropped.length) void invoke('complete_desktop_drop', { paths: dropped, page });
        setActive(false);
        setMode(null);
        setPaths([]);
        pathsRef.current = [];
        receivedFiles.current = false;
      }
    }).then(fn => { stopDrop = fn; });
    listen<string>('desktop-drag-session', event => {
      if (event.payload === 'start') setActive(true);
      if (event.payload === 'cancel' || event.payload === 'end') {
        if (!receivedFiles.current) { setActive(false); setMode(null); }
      }
    }).then(fn => { stopSession = fn; });
    return () => { stopDrop?.(); stopSession?.(); document.documentElement.style.background = ''; document.body.style.background = ''; };
  }, []);

  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => {
      void invoke<Keys>('drop_modifiers').then(keys => {
        const next = modeFrom(keys);
        setMode(next);
        if (next) {
          const safeX = Math.max(8, Math.min(330, window.innerWidth / 2 - 8));
          const safeY = Math.max(8, Math.min(260, window.innerHeight / 2 - 8));
          setAnchor(point => ({ x: Math.max(safeX, Math.min(window.innerWidth - safeX, point.x)), y: Math.max(safeY, Math.min(window.innerHeight - safeY, point.y)) }));
        }
      }).catch(() => undefined);
    }, 30);
    return () => window.clearInterval(id);
  }, [active]);

  const currentItems = mode ? items[mode] : [];
  const modeName = mode === 'convert' ? 'DÖNÜŞTÜR' : mode === 'tools' ? 'ARAÇLAR' : 'DOSYA';
  return <main className="drag-overlay-root" aria-label="Tamarillo masaüstü sürükleme çarkı">
    {active && mode && paths.length > 0 && <section className="global-drag-wheel" style={{ left: anchor.x, top: anchor.y }}>
      <header><b>{modeName}</b><span>{paths.length} dosya · bırakmak için bir dilimin üstüne sürükle</span></header>
      <div className="wheel-grid">
        <div className="wheel-core"><span>t</span><small>TAMARILLO</small></div>
        {currentItems.map((item, index) => {
          const angle = (360 * index / Math.max(currentItems.length, 1)) - 90;
          return <div key={item.page} data-page={item.page} className={hover === index ? 'global-petal hovered' : 'global-petal'} style={{ '--angle': `${angle}deg`, '--reverse-angle': `${-angle}deg` } as CSSProperties}>
            <b>{item.title}</b><small>{item.detail}</small>
          </div>;
        })}
      </div>
      <footer>{paths[0]?.split(/[\\/]/).pop()} · {Math.round(cursor.x)}, {Math.round(cursor.y)}</footer>
    </section>}
  </main>;
}
