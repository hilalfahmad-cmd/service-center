# Service Center

GUI GTK4/libadwaita (Rust) untuk mendaftarkan program sebagai **layanan systemd
user**, lalu memantau dan mengontrolnya dari satu tempat.

## Fitur

- Daftar terpisah untuk layanan yang didaftarkan manual dan unit layanan yang
  ditemukan dari `systemd --user`, lengkap dengan status.
- Kontrol start/stop/restart dan log untuk layanan manual maupun unit lain yang
  dimuat systemd. Unit yang ditemukan otomatis tidak diedit, di-enable/disable,
  atau dihapus oleh aplikasi.
- Pemilih file dengan deteksi perintah otomatis:
  - `.py`: memakai `venv/.venv` di sekitarnya bila ada, jika tidak `python3`.
  - `.js/.mjs/.cjs`: dijalankan dengan `node`.
  - `package.json`: `npm start` bila ada `scripts.start`, jika tidak `node` pada `main`/`index.js`.
  - `.sh/.bash`, `.jar`, atau binary yang executable.
- **Edit layanan**: ubah file program, perintah, direktori kerja, environment,
  kebijakan restart, dan autostart tanpa menghapus layanan.
- Mulai / hentikan / restart, switch autostart (enable/disable), dan log via
  `journalctl --user`.
- Restore database PostgreSQL dari dump `.sql`, `.dump`, `.backup`, atau `.tar`
  langsung dari halaman detail layanan.
- **Cloudflare Tunnel**: atur Tunnel ID, file credentials, dan ingress dari aplikasi.
- Banner untuk mengaktifkan **linger** agar layanan jalan saat boot tanpa login.

## Persyaratan

- systemd dengan user session (`systemctl --user`).
- GTK4 >= 4.10 dan libadwaita >= 1.5 (misalnya Fedora 40+ atau Ubuntu 24.04+).
- Opsional: `cloudflared` untuk Cloudflare Tunnel, `psql`/`pg_restore` untuk restore database.

## Pasang

### Dari paket rilis

Unduh dari halaman [Releases](https://github.com/Hilalfajar/service-center/releases).

Fedora / RHEL:

```bash
sudo dnf install ./service-center-1.2.0-1.x86_64.rpm
```

Debian / Ubuntu:

```bash
sudo apt install ./service-center_1.2.0-1_amd64.deb
```

Arsip tar.gz (tanpa root, ke `~/.local`):

```bash
tar -xzf service-center-1.2.0-x86_64.tar.gz
cd service-center-1.2.0
./install.sh
```

### Build dari source

```bash
git clone https://github.com/Hilalfajar/service-center.git
cd service-center
./install.sh
```

`install.sh` menjalankan `cargo build --release`, lalu memasang binary ke
`~/.local/bin/service-center`, entri menu, dan ikon.

## Cloudflare Tunnel

1. Buka menu **≡ → Cloudflare Tunnel…**
2. Jika `cloudflared` belum tersedia, klik **Unduh** untuk memasang binary resmi
   Linux x86_64/ARM64 ke `~/.local/bin` (memerlukan `curl`).
3. Isi Tunnel ID/nama dan pilih file credentials `.json`.
4. Isi ingress, satu baris per hostname:

   ```text
   app.example.com = http://localhost:8080
   ```

5. Simpan. Aplikasi menulis `~/.cloudflared/config.yaml` dan mendaftarkan
   `cloudflared` sebagai layanan Service Center.

DNS record tetap harus dibuat terpisah, lewat dashboard (Zero Trust → Tunnels →
Public Hostname) atau CLI:

```bash
cloudflared tunnel route dns <tunnel> app.example.com
```

Ingress di `config.yaml` hanya mengatur routing; tanpa DNS record, hostname
tidak akan resolve.

Layanan yang sudah didaftarkan manual dapat dipilih dari bagian **Didaftarkan
manual** di sidebar lalu diedit lewat tombol **Edit**. Bagian **Layanan systemd
pengguna** menampilkan unit yang dimuat oleh `systemd --user`; aplikasi dapat
mengontrol status aktifnya dan membaca log, tetapi tidak mengubah konfigurasi,
autostart, atau file unitnya. Service Center hanya menemukan unit dalam user
manager, bukan unit system-wide yang dikelola root.

## Restore database

1. Pilih layanan di sidebar.
2. Klik **Restore** pada bagian **Database**.
3. Isi host, port, user, password, dan nama database tujuan.
4. Pilih file dump, lalu **Start** dan pantau log.

Format `.sql` diproses dengan `psql`; `.dump`, `.backup`, dan `.tar` dengan
`pg_restore`. Password hanya dipakai selama proses dan tidak disimpan.
Opsi **Kosongkan objek lama** menghapus skema `public` sebelum restore.

## Data

- Unit systemd: `~/.config/systemd/user/sc-<id>.service`
- Registry layanan: `~/.local/share/service-center/services.json`
- Config cloudflared: `~/.cloudflared/config.yaml`

Menghapus layanan dari aplikasi tidak menghapus file programnya.

## Uninstall

Paket sistem:

```bash
sudo dnf remove service-center
```

```bash
sudo apt remove service-center
```

Instalasi lewat skrip:

```bash
rm -f ~/.local/bin/service-center \
      ~/.local/share/applications/service-center.desktop
rm -rf ~/.local/share/icons/hicolor/*/apps/service-center.*
```

Hapus `~/.local/share/service-center` bila registry layanan juga tidak diperlukan.
