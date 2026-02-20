//! Nomor syscall Chilena
//!
//! Digunakan oleh kernel dispatcher dan userspace library.
//! Konvensi: angka kecil = operasi fundamental.

pub const EXIT:    usize = 0x01; // Keluar dari proses
pub const SPAWN:   usize = 0x02; // Buat proses baru dari ELF
pub const READ:    usize = 0x03; // Baca dari handle
pub const WRITE:   usize = 0x04; // Tulis ke handle
pub const OPEN:    usize = 0x05; // Buka file/device
pub const CLOSE:   usize = 0x06; // Tutup handle
pub const STAT:    usize = 0x07; // Info metadata file
pub const DUP:     usize = 0x08; // Duplikasi handle
pub const REMOVE:  usize = 0x09; // Hapus file
pub const HALT:    usize = 0x0A; // Halt/reboot sistem
pub const SLEEP:   usize = 0x0B; // Tidur N detik
pub const POLL:    usize = 0x0C; // Cek kesiapan handle
pub const ALLOC:   usize = 0x0D; // Alokasi memori userspace
pub const FREE:    usize = 0x0E; // Bebaskan memori userspace
pub const KIND:    usize = 0x0F; // Tipe handle (file/device/socket)
pub const SEND:    usize = 0x10; // Kirim pesan ke proses lain (block sampai diterima)
pub const RECV:    usize = 0x11; // Tunggu pesan masuk (block sampai ada)
