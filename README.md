# Tamarillo

İçerik üreticileri için Windows masaüstü araç seti. `Copyright EgeHS Medya 2026 - 2027`

## Çalışan araçlar

- **Medya:** MP4 sıkıştırma ve dönüştürme, MOV/WebM çıktı, MP3/WAV ses ayırma, videodaki sessiz aralıkları analiz edip kesme.
- **İndirme:** yt-dlp ile desteklenen herkese açık bağlantıdan en iyi video kalitesi veya MP3 ses indirme.
- **Altyazı ve özet:** YouTube altyazısı / otomatik altyazısı indirme; Whisper.cpp tabanlı yerel ses/video transkripsiyonu; metin üzerinde cihaz içi, extractive özet oluşturma.
- **Görseller:** Toplu yeniden boyutlandırma, JPEG sıkıştırma, PNG/JPG/WebP dönüştürme ve görsel filigran.
- **Üretim:** Cihazda saklanan senaryo notları, yerel hook fikir şablonları ve odak sayacı.
- **Kısayollar:** Araç çarkı, dosya seçici, arama, işlem geçmişi ve uygulama arka plandayken bağlantı yakalama.

> İlk yerel transkripsiyonda uygulama Hugging Face üzerinden yaklaşık 142 MB'lık çok dilli Whisper base modelini indirir. Model bilgisayarda saklanır; konuşma tanıma ve özetleme cihazda yapılır. YouTube altyazısı indirme yalnızca video altyazı sağlıyorsa çalışır.

Dosyalar varsayılan olarak yerelde işlenir. Video/ses ve görsel çıktıları kaynak dosyanın yanında yeni dosyalar olarak oluşturulur; indirmeler Windows İndirilenler klasörüne gider. Notlar ve işlem geçmişi uygulamanın yerel tarayıcı depolamasında kalır.

## Windows kısayolları

| Kısayol | İşlem |
| --- | --- |
| `Ctrl + Shift + D` | Uygulama arka plandayken panodaki bağlantıyı Tamarillo'ya getir |
| `Ctrl + Space` | Araç seçme çarkını aç |
| `Ctrl + O` | Dosya seç |
| `Ctrl + F` | Araçlarda ara |
| `Ctrl + 2` | Son işlemler |
| `↑ / ↓ / ← / →` | Çarkta araç seç |
| `Home / End` | İlk / son araca git |
| `Enter / Space` | Seçili aracı aç |
| `Esc / Tab` | Çarkı kapat |
| `Shift` + dosya bırakma | Dönüştürme çarkı |
| `Ctrl` + dosya bırakma | Araç çarkı |
| `Alt` + dosya bırakma | Dosya işlemleri |

Windows Explorer sürükle-bırak olayları bazı sistemlerde basılı modifier tuşunu uygulamaya iletmez. Bu durumda dosya bırakıldıktan sonra araç seçme penceresi görünür.

## Teknoloji ve geliştirme

Tauri 2, React, TypeScript ve Rust. Windows için NSIS kurulum sihirbazı üretir. Gereksinimler: Node.js 22+, Rust stable/MSVC, Microsoft C++ Build Tools ve WebView2 Runtime.

```powershell
npm install
npm run tauri dev
```

```powershell
npm run tauri:build
```

NSIS yükleyicisi `src-tauri/target/release/bundle/nsis/` altında oluşur. GitHub Actions FFmpeg, ffprobe, yt-dlp ve Whisper.cpp Windows araçlarını indirip uygulama paketine dahil eder. Whisper modeli ilk kullanımdaki uygulama çalıştırmasında indirilir. Üçüncü taraf araçların lisans ve kaynak bağlantıları yayın notlarında yer almalıdır.
