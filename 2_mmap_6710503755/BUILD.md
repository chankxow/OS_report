# BUILD — 2_mmap (Memory map visualizer with Rust)

## สิ่งที่ต้องมี
- Linux ที่มี `/proc` (ทดสอบกับ Ubuntu 24.04, kernel 7.0) บน Windows และ macOS ดูหัวข้อ "ระบบปฏิบัติการอื่น"
- Rust toolchain: `rustc` + `cargo` เวอร์ชัน 1.75 ขึ้นไป (ทดสอบกับ 1.75.0)
  ```sh
  sudo apt install cargo        # Ubuntu/Debian
  # หรือติดตั้งผ่าน https://rustup.rs
  ```
- อินเทอร์เน็ตสำหรับการคอมไพล์ครั้งแรก เพื่อดาวน์โหลด crate (`ratatui`, `libc` และ dependency ของมัน)
  ตามเวอร์ชันที่ล็อกไว้ใน `Cargo.lock` ครั้งต่อไปใช้ cache ใน `~/.cargo` ได้เลย
- เทอร์มินัลที่รองรับสี (โหมด live ใช้ 24-bit color ถ้ามี)

## คอมไพล์
```sh
make            # = cargo build --release --locked แล้วคัดลอก target/release/mmap มาเป็น ./mmap
make test       # = cargo test --locked  (unit test 10 ตัว: parse /proc, จัดประเภท region, วางแผนที่)
make clean      # ลบ target/ และ ./mmap
```
ถ้าไม่มี `cargo` คำสั่ง `make` จะหยุดพร้อมบอกวิธีติดตั้ง การคอมไพล์ครั้งแรกใช้เวลาประมาณ 15–60 วินาที
ขึ้นกับเครื่องและความเร็วอินเทอร์เน็ต

## ระบบปฏิบัติการอื่น
โปรแกรมนี้อ่านข้อมูลจาก `/proc/PID/maps` และ `/proc/PID/smaps` ซึ่งเป็นกลไกของ Linux kernel โดยเฉพาะ
Windows และ macOS ไม่มี `/proc` (ใช้ API คนละแบบ เช่น `VirtualQueryEx` บน Windows และ `mach_vm_region` บน macOS)
จึงรันบนสองระบบนี้โดยตรงไม่ได้ ถ้าสั่ง `make` บนระบบที่ไม่ใช่ Linux จะหยุดพร้อมบอกวิธีด้านล่าง ให้รันผ่าน Linux แทน

| ระบบ | วิธี | สิ่งที่เห็น |
|---|---|---|
| Linux | `make` แล้ว `./mmap` | ทุกโปรเซสของผู้ใช้ |
| Windows 10/11 | WSL2 (Ubuntu) ขั้นตอนเดียวกับ Linux | โปรเซสภายใน WSL2 (ไม่ใช่โปรแกรม Windows) |
| macOS | `make docker` (ต้องมี Docker Desktop) | โปรเซสภายใน container จึงใช้ `--demo` เป็นค่าเริ่มต้น |

**Windows: WSL2**
```powershell
wsl --install            # ใน PowerShell (Administrator) แล้วรีสตาร์ตเครื่อง
```
จากนั้นเปิด Ubuntu แล้วทำตามขั้นตอนของ Linux
```sh
sudo apt update && sudo apt install cargo
cd /mnt/c/Users/<ชื่อผู้ใช้>/Downloads/os_report_6710503755/2_mmap_6710503755   # ไดรฟ์ C: อยู่ที่ /mnt/c
make
./mmap --demo
```
ถ้าคอมไพล์ใต้ `/mnt/c` ช้า ให้คัดลอกโฟลเดอร์ไปไว้ใน home ของ WSL ก่อน (`cp -r ... ~/`)

**macOS: Docker**

ติดตั้ง [Docker Desktop](https://www.docker.com/products/docker-desktop/) แล้วเปิดโปรแกรมไว้ จากนั้น
```sh
make docker                                  # คอมไพล์ใน container (rust:1.75) แล้วเปิด ./mmap --demo แบบเวลาจริง
make docker ARGS="--demo --snapshot"         # รายงานแบบข้อความครั้งเดียว
```
- โฟลเดอร์งานถูก mount แบบอ่านอย่างเดียว ไฟล์ที่คอมไพล์เก็บใน Docker volume `mmap-cargo` และ `mmap-target`
  ครั้งต่อไปจึงไม่ต้องดาวน์โหลดและคอมไพล์ใหม่ทั้งหมด ลบได้ด้วย `docker volume rm mmap-cargo mmap-target`
- Docker Desktop บน macOS รัน Linux VM อยู่เบื้องหลัง โปรแกรมจึงเห็นหน่วยความจำของโปรเซสใน VM นั้น ไม่ใช่ของโปรแกรม macOS
- คำสั่งเดียวกันใช้ได้บน Windows ที่มี Docker Desktop และ `make` (เช่นใน Git Bash) แต่ WSL2 ง่ายกว่า

## รัน
```sh
./mmap -h                          # ดูตัวเลือกทั้งหมด
./mmap                             # เปิดหน้าเลือกโปรเซส แล้วเข้าสู่หน้าจอเวลาจริง
./mmap -p 1234                     # ดูโปรเซส PID 1234 แบบเวลาจริง
./mmap --demo                      # รันโปรเซสตัวอย่างที่จอง/คืนหน่วยความจำ แล้วดูแบบเวลาจริง
./mmap -p $$ --snapshot            # รายงานแบบข้อความครั้งเดียวของ shell ปัจจุบัน
./mmap -p $$ --snapshot --no-color > snapshot.txt   # บันทึกเป็นข้อความล้วน
./mmap --demo --snapshot           # รอ demo จองหน่วยความจำ ~6 วินาที แล้วพิมพ์รายงาน
```

| ตัวเลือก | ความหมาย | ค่าเริ่มต้น |
|---|---|---|
| `-p PID`, `--pid PID` | โปรเซสที่ต้องการดู (โปรเซสของผู้ใช้เอง; ของผู้ใช้อื่นต้องใช้ root) | ไม่ระบุ = เปิดหน้าเลือกโปรเซส |
| `-s`, `--snapshot` | พิมพ์รายงานแบบข้อความครั้งเดียวแล้วจบ (ต้องใช้คู่กับ `-p` หรือ `--demo`) | ปิด (โหมด live) |
| `--demo` | สร้างโปรเซสตัวอย่างแล้วดูหน่วยความจำของมัน (ใช้คู่กับ `-p` ไม่ได้) | ปิด |
| `-i MS`, `--interval MS` | ความถี่ในการอ่านข้อมูลใหม่ของโหมด live หน่วยมิลลิวินาที (200–10000) | 1000 |
| `-w N`, `--width N` | snapshot: จำนวนช่องของแผนที่ต่อบรรทัด (40–400) | 96 |
| `--no-color` | snapshot: ข้อความล้วนไม่มีสี ANSI (เป็นแบบนี้อัตโนมัติเมื่อตั้ง `NO_COLOR` หรือ redirect/pipe) | ปิด |
| `-h`, `--help` | แสดงวิธีใช้ | - |

โหมด live ต้องรันบนเทอร์มินัล ถ้า redirect หรือ pipe ออก โปรแกรมจะแจ้งให้ใช้ `--snapshot` แทน
อาร์กิวเมนต์ที่ผิดรูปแบบ อยู่นอกช่วง หรือ PID ที่ไม่มีอยู่ จะถูกปฏิเสธพร้อมข้อความอธิบายและ exit code 1

### ปุ่มในโหมด live
| ปุ่ม | การทำงาน |
|---|---|
| `↑`/`↓`, `j`/`k`, `PgUp`/`PgDn`, `g`/`G` | เลื่อนเลือก region |
| `Enter` | ดูรายละเอียดของ region ที่เลือก (ค่าจาก `smaps`) |
| `s` / `r` | เปลี่ยนคอลัมน์ที่ใช้เรียง / กลับลำดับ |
| `f` | กรองตามชนิดของ region |
| `t` หรือ `Tab` | สลับมุมมองรายการ region กับสรุปตามชนิด |
| `p` | เลือกโปรเซสใหม่ (พิมพ์เพื่อค้นหา, `Enter` เลือก, `Esc` ยกเลิก) |
| `Space` | หยุด/เล่นการอัปเดต |
| `+` / `-` | เพิ่ม/ลดความถี่ในการอัปเดต |
| `u` | อ่านข้อมูลใหม่ทันที |
| `?`, `h`, `F1` | แสดงหน้าช่วยเหลือ |
| `q`, `Esc`, `Ctrl+C` | ออกจากโปรแกรม |

ใช้เมาส์ได้ (ล้อเลื่อนและคลิกเลือกแถว)

## ข้อมูลที่อ่านและความปลอดภัย
- อ่านอย่างเดียวจาก `/proc/PID/maps`, `/proc/PID/smaps`, `/proc/PID/status`, `/proc/PID/cmdline` และ `/proc/meminfo`
  ไม่เขียนหรือแก้ไขหน่วยความจำของโปรเซสอื่น ไม่ต้องใช้สิทธิ์ root และไม่เปลี่ยนการตั้งค่าใดๆ ของระบบ
- โปรเซส `--demo` ใช้หน่วยความจำไม่เกิน ~140 MiB, map ไฟล์ระบบแบบอ่านอย่างเดียว, ถูก kill ทันทีเมื่อ `mmap` ออก
  (รวมกรณี error หรือ panic), ตั้ง `PR_SET_PDEATHSIG` ไว้จึงจบเองถ้าโปรเซสแม่ตายกะทันหัน และหยุดเองหลัง 30 นาทีเป็นอย่างช้า
- ตรวจได้ว่าไม่มีโปรเซส demo ค้างหลังออกจากโปรแกรม:
  ```sh
  pgrep -af demo-child || echo "no leftover demo child"
  ```

## ปัญหาที่พบบ่อยและวิธีแก้
| อาการ | สาเหตุ | วิธีแก้ |
|---|---|---|
| `error: mmap reads Linux /proc and only runs on Linux.` | สั่ง `make` บน macOS หรือ Windows | Windows: ใช้ WSL2 / macOS: `make docker` |
| `error: cargo not found` | ยังไม่ได้ติดตั้ง Rust | `sudo apt install cargo` หรือ https://rustup.rs |
| คอมไพล์ครั้งแรกล้มเหลวด้วย `failed to download` / `Couldn't resolve host` | ไม่มีอินเทอร์เน็ต จึงดาวน์โหลด crate ไม่ได้ | ต่ออินเทอร์เน็ตแล้ว `make` ใหม่ (ครั้งต่อไปใช้ cache) |
| `package … cannot be built because it requires rustc 1.xx` | `Cargo.lock` ถูกลบหรือถูกอัปเดตด้วย `cargo update` | ใช้ `Cargo.lock` ต้นฉบับ หรืออัปเดต Rust ผ่าน rustup |
| `mmap: stdout is not a terminal; use --snapshot for a text report` | โหมด live ถูก redirect/pipe | รันในเทอร์มินัลโดยตรง หรือใช้ `--snapshot` |
| `mmap: no process with PID …` | โปรเซสไม่มีอยู่หรือจบไปแล้ว | ดู PID ด้วย `ps -u $USER` หรือเลือกจากหน้าเลือกโปรเซส (`./mmap`) |
| `warning: permission denied: the memory map of PID … is readable only by its owner or root` | เป็นโปรเซสของผู้ใช้อื่น kernel จึงไม่ให้อ่าน `smaps` | เลือกโปรเซสของตัวเอง (ในหน้าเลือกโปรเซส แถวสีจางคือของผู้ใช้อื่น) |
| `terminal too small: …x… (need at least 70x22)` | หน้าต่างเทอร์มินัลเล็กเกินไป | ขยายหน้าต่างหรือลดขนาดตัวอักษร |
| สีเพี้ยนหรือเส้นกรอบเป็นตัวอักษรแปลกๆ | เทอร์มินัลไม่รองรับ UTF-8 หรือสี 24-bit | Windows: ใช้ Windows Terminal เปิด WSL / macOS: ใช้ iTerm2 หรือใช้ `--snapshot --no-color` |
| `Cannot connect to the Docker daemon` | Docker Desktop ยังไม่ได้เปิด | เปิด Docker Desktop รอจนขึ้นว่า running แล้วสั่งใหม่ |
| `the input device is not a TTY` | `make docker` ถูกรันจากสคริปต์หรือ pipe ไม่ใช่เทอร์มินัล | รันจากหน้าต่างเทอร์มินัลโดยตรง |
| ใน Docker เห็นแค่โปรเซสไม่กี่ตัว | container มองเห็นเฉพาะโปรเซสของตัวเอง | เป็นพฤติกรรมปกติ ใช้ `--demo` (ค่าเริ่มต้นของ `make docker`) |
| คอมไพล์ใน WSL2 ช้ามาก | โฟลเดอร์อยู่ใต้ `/mnt/c` ซึ่งเข้าถึงข้ามระบบไฟล์ | `cp -r 2_mmap_6710503755 ~/` แล้วคอมไพล์ใน `~` |

## สิ่งที่ห้ามทำ
- **ห้ามรันด้วย `sudo` เพื่อดูโปรเซสทั่วไป** โปรแกรมอ่านโปรเซสของตัวเองได้โดยไม่ต้องใช้ root
  การใช้ root จะเปิดให้เห็นหน่วยความจำของทุกโปรเซสในเครื่อง ซึ่งเกินความจำเป็นของงานนี้
- **ห้ามลบ `Cargo.lock` หรือสั่ง `cargo update`** เพราะ crate รุ่นใหม่อาจไม่รองรับ Rust 1.75 แล้วคอมไพล์ไม่ผ่าน
- **ห้ามแก้ `make docker` ให้ mount โฟลเดอร์แบบเขียนได้ หรือเพิ่ม `--privileged` / `--pid=host`**
  คำสั่งตั้งใจให้ container อ่านซอร์สได้อย่างเดียวและเห็นเฉพาะโปรเซสของตัวเอง
- **ห้ามปิดเทอร์มินัลด้วย `kill -9` ขณะใช้ `--demo` เป็นนิสัย** ปกติให้กด `q` เพื่อให้โปรแกรมคืนสถานะเทอร์มินัลและปิด demo
  (ถ้าถูก kill จริง demo จะจบเองจาก `PR_SET_PDEATHSIG` แต่เทอร์มินัลอาจค้างในโหมดเต็มจอ แก้ด้วยคำสั่ง `reset`)
- **ห้ามส่ง `target/` หรือ `./mmap` ไว้ในโฟลเดอร์ส่งงาน** (`target/` มีขนาดหลายร้อย MB และโจทย์หักคะแนนเมื่อมีไฟล์เกิน)

## สิ่งที่ควรทำ
- รัน `make test` หลังคอมไพล์บนเครื่องใหม่ (ต้องผ่าน 10/10)
- เริ่มจาก `./mmap --demo` เพื่อดูว่าหน้าจอเปลี่ยนตามการจอง/คืนหน่วยความจำ ก่อนดูโปรเซสอื่น
- ใช้ `./mmap -p PID --snapshot --no-color > snapshot.txt` เมื่อต้องการเก็บผลไว้อ้างอิงในรายงาน
- ออกจากโปรแกรมด้วย `q` แล้วตรวจว่าไม่มี demo ค้าง (`pgrep -af demo-child`)
- รัน `make clean` ก่อนบีบอัดหรือส่งงาน และหลังใช้ Docker เสร็จให้ลบ cache ด้วย `docker volume rm mmap-cargo mmap-target`
