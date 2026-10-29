# Tamarillo

Windows masaüstü araç seti. Copyright EgeHS Medya 2026 - 2027

## Teknoloji

Tauri 2, React, TypeScript ve Rust. Windows için NSIS kurulum sihirbazı üretir; `installMode: both` kullanıcıya bu bilgisayar veya yalnızca kendi hesabı için kurulum seçeneği verir. Kurulum Başlat menüsüne ve Windows'un yüklü uygulamalar listesine kayıt ekler.

## Geliştirme

Gereksinimler: Node.js 22+, Rust stable/MSVC, Microsoft C++ Build Tools ve WebView2 Runtime.

```powershell
npm install
npm run tauri dev
```

Windows kurulum dosyası üretmek için:

```powershell
npm run tauri:build
```

NSIS yükleyicisi `src-tauri/target/release/bundle/nsis/` altında oluşur. `v*` biçiminde GitHub etiketi gönderildiğinde workflow Windows üzerinde paketi derler ve GitHub Release'e yükler. GitHub Actions her derlemede FFmpeg, ffprobe ve yt-dlp Windows araçlarını indirip NSIS paketine dahil eder. FFmpeg derlemesi GPL codec bileşenleri içerebilir; dağıtılan sürümün üçüncü taraf lisans ve kaynak bağlantıları Release notlarında ayrıca sunulmalıdır. İlk sürümde dosya dönüştürme ve URL indirme komutları kullanılabilir; tam ilerleme çubuğu, iptal ve kuyruk sonraki iterasyondadır.

## Telif

Uygulama arayüzü, yerel uygulama bilgisi ve kurulum metadata'sı `Copyright EgeHS Medya 2026 - 2027` satırını taşır.
