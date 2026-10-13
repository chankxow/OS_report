# OS Report 2026

- ชื่อ-นามสกุล: เกียรติภูมิ พละสาร
- รหัสนิสิต: 6710503755
- รายวิชา: 01204332 ระบบปฏิบัติการ (OS) มหาวิทยาลัยเกษตรศาสตร์ ปี 3 ภาคเรียนที่ 1 ปีการศึกษา 2569

> โปรแกรมทั้งหมดในโฟลเดอร์นี้ทำขึ้นเพื่อการศึกษาในรายวิชาระบบปฏิบัติการเท่านั้น ไม่ใช่โปรแกรมสำหรับนำไปใช้งานจริง

## โครงสร้าง
```
os_report_6710503755/
├── 1_parallel_6710503755/   ข้อ 1: การคูณเมตริกซ์แบบขนานด้วย C + OpenMP
│   ├── parallel.c           โปรแกรมหลัก (ไฟล์เดียว)
│   ├── bench.sh             สคริปต์รันการทดลองทั้งหมด → CSV
│   ├── plot.py              สร้างกราฟและวิเคราะห์ Amdahl's law จาก CSV
│   ├── Makefile             `make` = คอมไพล์เป็น ./parallel
│   └── BUILD.md             วิธีคอมไพล์ รัน และทำการทดลองซ้ำ
├── 2_mmap_6710503755/       ข้อ 2: เครื่องมือแสดงภาพ memory map ของโปรเซส (Rust)
│   ├── mmap.rs              โปรแกรมหลัก: อ่านตัวเลือก แล้วเลือกโหมด (live / snapshot / demo)
│   ├── procfs.rs            อ่านและแยกข้อมูลจาก /proc (อ่านอย่างเดียว)
│   ├── model.rs             ชนิดของ region, สรุปตามชนิด, การจัดวางแผนที่ address space
│   ├── ui.rs                หน้าจอเต็มจอ (TUI) สไตล์ btop
│   ├── snapshot.rs          รายงานแบบข้อความครั้งเดียว
│   ├── demo.rs              โปรเซสตัวอย่างที่จอง/คืนหน่วยความจำเป็นจังหวะ
│   ├── Cargo.toml, Cargo.lock
│   ├── Makefile             `make` = คอมไพล์เป็น ./mmap
│   └── BUILD.md             วิธีคอมไพล์ รัน และปุ่มควบคุม
├── README.md
└── report_6710503755.pdf    รายงาน (ผลการทดลองและการวิเคราะห์ทั้งหมด)
```

รายละเอียดวิธีคอมไพล์ รัน ปัญหาที่พบบ่อย และสิ่งที่ห้ามทำ อยู่ใน `BUILD.md` ของแต่ละข้อ
ผลการทดลองและการวิเคราะห์อยู่ในรายงาน `report_6710503755.pdf`

## ระบบที่รองรับ
ทั้งสองข้อพัฒนาและทดสอบบน Linux (Ubuntu 24.04) ผลการทดลองในรายงานวัดบน Linux ทั้งหมด

| | Linux | Windows 10/11 | macOS |
|---|---|---|---|
| ข้อ 1 (`parallel`) | `make` | ใช้ WSL2 แล้ว `make` | `brew install libomp` แล้ว `make` (ค่า Peak RSS/VmPeak ไม่ถูกต้อง) |
| ข้อ 2 (`mmap`) | `make` | ใช้ WSL2 แล้ว `make` | `make docker` (ต้องมี Docker Desktop) |

- Windows/macOS ทดสอบเฉพาะการคอมไพล์ในสภาพแวดล้อมจำลอง (container และการจำลองค่า `uname`) ยังไม่ได้ทดสอบบนเครื่อง Windows หรือ Mac จริง
- ถ้าสั่ง `make` บนระบบที่ไม่รองรับ `Makefile` จะหยุดพร้อมบอกวิธีที่ถูกต้อง

---

## ข้อ 1 — Intro to Parallel Programming (C + OpenMP)

`parallel.c` คำนวณ C = A × B สำหรับเมตริกซ์จัตุรัส n × n (`double`) แบบขนานด้วย OpenMP
โดยจับเวลาแยกช่วง serial และ parallel เพื่อวิเคราะห์ด้วย Amdahl's law
`bench.sh` รันการทดลองหลายมิติ (จำนวน thread, ขนาด n, การตั้งค่าโปรแกรม, การตั้งค่า OS, หน่วยความจำ) และ `plot.py` สร้างกราฟกับตารางสรุป

### สิ่งที่ต้องมี
| ใช้สำหรับ | สิ่งที่ต้องติดตั้ง |
|---|---|
| คอมไพล์และรัน | `gcc` ที่รองรับ OpenMP (ทดสอบกับ gcc 13.3) |
| รัน `bench.sh` | `bash`, `taskset`, `/usr/bin/time`, `timeout`, `nice`, `lscpu` |
| รัน `plot.py` | Python 3 + `matplotlib` |

```sh
sudo apt install build-essential time python3-matplotlib
```

### คอมไพล์และรัน
```sh
cd 1_parallel_6710503755
make                                            # gcc -O2 -Wall -Wextra -fopenmp -o parallel parallel.c -lm
./parallel -h                                   # แสดงตัวเลือกทั้งหมด
./parallel -n 800 -t 8 -a ikj -v                # n=800, 8 threads, ตรวจความถูกต้องของคำตอบ
./parallel -n 1600 -t 4 -a blocked -b 64        # อัลกอริทึม blocked, tile 64×64
./parallel -n 400 -t 8 -s dynamic -c 4          # OpenMP schedule แบบ dynamic, chunk 4
./parallel -o csv -H -n 800 -t 8 -l mytest      # ผลลัพธ์เป็น CSV พร้อมหัวตาราง
make clean
```

| ตัวเลือก | ความหมาย | ค่าเริ่มต้น |
|---|---|---|
| `-n N` | ขนาดเมตริกซ์ n×n (1–4096) | 800 |
| `-t T` | จำนวน OpenMP threads (1 ถึง 4 × จำนวน CPU) | `OMP_NUM_THREADS` หรือจำนวน CPU |
| `-a ALGO` | `naive` \| `ikj` \| `transpose` \| `blocked` | `ikj` |
| `-b BS` | ขนาด tile ของ `blocked` | 64 |
| `-s SCHED` | `static` \| `dynamic` \| `guided` | `static` |
| `-c CHUNK` | chunk size ของ schedule (0 = ค่า default ของ OpenMP) | 0 |
| `-r R` | ทำ kernel ซ้ำ R รอบ (1–100) รายงานเวลาที่ดีที่สุดและเฉลี่ย | 3 |
| `-v` | ตรวจผลกับการคำนวณแบบ serial (relative error ≤ 1e-9) | ปิด |
| `-l LABEL` | ข้อความกำกับในคอลัมน์แรกของ CSV | `-` |
| `-o FORMAT` | `table` \| `csv` | `table` ถ้าแสดงบนเทอร์มินัล, `csv` ถ้า redirect/pipe |
| `-H` | พิมพ์หัวตาราง CSV | ปิด |

ค่าที่ไม่ถูกต้อง (เช่น `-n 5000`) จะถูกปฏิเสธพร้อม error และจบด้วย exit code 1 ถ้าใช้ `-v` แล้วผลตรวจไม่ผ่านก็จบด้วย exit code 1

ปรับพฤติกรรมของ OpenMP / OS จากภายนอกได้ เช่น
```sh
OMP_PROC_BIND=close OMP_PLACES=cores ./parallel -n 1600 -t 8   # ผูก thread กับ core
OMP_WAIT_POLICY=passive ./parallel -n 400                       # thread ที่รอจะหลับแทนการ spin
taskset -c 0-3 ./parallel -n 1600 -t 8                          # จำกัดให้ใช้ได้แค่ CPU 0–3
nice -n 19 ./parallel -n 1600                                   # ลดลำดับความสำคัญของโปรเซส
```

### อัลกอริทึม
ทุกอัลกอริทึมแบ่งงานแบบขนานที่ลูปนอกสุด แต่ละช่องของ C ถูกเขียนโดย thread เดียว จึงไม่ต้องใช้ lock หรือ atomic

| `-a` | วิธีการ |
|---|---|
| `naive` | ลูป i-j-k ตามนิยาม (อ่าน B ตามคอลัมน์ ใช้ cache ไม่ดี) |
| `ikj` | สลับลูปเป็น i-k-j (อ่าน B และเขียน C ตามแถว เป็นมิตรกับ cache) |
| `transpose` | transpose B ก่อน แล้วคิดแต่ละช่องเป็น dot product ของสองแถว (ใช้หน่วยความจำเพิ่ม n²) |
| `blocked` | i-k-j แบบแบ่ง tile BS×BS |

A และ B สร้างจาก LCG ที่ seed คงที่ ทุกการรันจึงได้ checksum เท่ากัน ใช้เทียบความถูกต้องข้ามอัลกอริทึมและจำนวน thread ได้

### ผลลัพธ์ของโปรแกรม
จับเวลาแต่ละช่วงด้วย `omp_get_wtime()`: `t_init` (serial), `t_mul_best` / `t_mul_mean` (parallel), `t_post` (serial, checksum), `t_verify` (serial, เฉพาะ `-v`)
แบบตารางแสดง Configuration / Timing / Results ส่วนแบบ CSV พิมพ์ 1 บรรทัดต่อการรัน 1 ครั้ง

```
label,algo,n,threads,schedule,chunk,block,t_init,t_mul_best,t_mul_mean,t_post,t_verify,t_total,gflops,mem_theory_mb,maxrss_mb,vmpeak_mb,checksum,max_rel_err,verified
```
`verified`: 1 = ถูกต้อง, 0 = ผิด, -1 = ไม่ได้ตรวจ

### ทำการทดลองซ้ำ
```sh
cd 1_parallel_6710503755
./bench.sh ~/bench_results quick    # รุ่นย่อ ~15 วินาที ไว้ทดสอบว่าสคริปต์ทำงานได้
./bench.sh ~/bench_results          # การทดลองเต็ม ~45–60 นาที
python3 plot.py ~/bench_results     # สร้างกราฟ (figures/) และ summary.md
```
- ควรระบุโฟลเดอร์ผลลัพธ์นอก `os_report_6710503755/` เพื่อไม่ให้มีไฟล์เกินในโฟลเดอร์ส่งงาน (ผลดิบไม่ได้รวมอยู่ในงานที่ส่ง)
- ระหว่างการทดลองควรปิดโปรแกรมอื่นและเสียบสายชาร์จ
- การทดลองครอบคลุม: จำนวน thread × ขนาด n, อัลกอริทึม, OpenMP schedule/chunk, block size, ระดับ optimization (`-O0`/`-O2`/`-O3 -march=native`),
  `OMP_PROC_BIND`/`OMP_PLACES`, CPU affinity, `OMP_WAIT_POLICY`, `nice`, และหน่วยความจำ (รวม `OMP_STACKSIZE`)
- `plot.py` ประมาณสัดส่วนส่วนที่ขนานได้ f ตาม Amdahl's law (f_kernel, f_program, f_phase) และ Karp–Flatt serial fraction ใช้เฉพาะ 2 ≤ p ≤ จำนวน physical core

### ความปลอดภัยต่อระบบ (DO NO HARM)
- จำกัด n ≤ 4096 (3 เมตริกซ์ ≈ 384 MiB) และ thread ≤ 4 เท่าของจำนวน CPU
- ตรวจผลของ `malloc` ทุกครั้ง และ `free` ก่อนจบโปรแกรม ตรวจค่า argument ทุกตัว
- `bench.sh` ไม่ต้องใช้ root และไม่แก้การตั้งค่าระบบ (governor ฯลฯ ถูกบันทึกเท่านั้น)
- โหลด CPU ในการทดลอง `nice` ถูกจำกัดเวลาด้วย `timeout 600` และถูก kill เมื่อสคริปต์จบหรือกด Ctrl+C

---

## ข้อ 2 — Memory Map Visualization (Rust)

`mmap` แสดงภาพการใช้หน่วยความจำของโปรเซสบน Linux โดยอ่านจาก `/proc/PID/maps`, `smaps`, `status` และ `/proc/meminfo` ได้ 2 แบบ
- **เวลาจริง** — หน้าจอเต็มจอสไตล์ btop อัปเดตทุก 1 วินาที (ปรับได้) ใช้คีย์บอร์ดและเมาส์
- **snapshot** — พิมพ์รายงานข้อความครั้งเดียวแล้วจบ redirect ลงไฟล์ได้

### สิ่งที่ต้องมี
| ใช้สำหรับ | สิ่งที่ต้องติดตั้ง |
|---|---|
| คอมไพล์ | Linux, `make`, `cargo` + `rustc` ≥ 1.75 |
| ดาวน์โหลด dependency | อินเทอร์เน็ตเฉพาะการคอมไพล์ครั้งแรก (crate `ratatui` และ `libc` ตาม `Cargo.lock`) |
| โหมดเวลาจริง | เทอร์มินัลอย่างน้อย 70 × 22 ตัวอักษร รองรับสี 24-bit |

```sh
sudo apt install build-essential cargo     # หรือติดตั้งผ่าน https://rustup.rs
```
`Cargo.lock` ล็อกเวอร์ชันให้คอมไพล์ได้กับ Rust 1.75 (เวอร์ชันใน apt ของ Ubuntu 24.04)

### คอมไพล์และรัน
```sh
cd 2_mmap_6710503755
make            # cargo build --release --locked แล้วคัดลอกเป็น ./mmap
make test       # unit test 10 ตัว
make clean      # ลบ target/ และ ./mmap

./mmap -h                                           # แสดงตัวเลือกทั้งหมด
./mmap                                              # เปิดหน้าเลือกโปรเซส แล้วเข้าสู่หน้าจอเวลาจริง
./mmap -p 1234                                      # ดูโปรเซส PID 1234
./mmap --demo                                       # รันโปรเซสตัวอย่าง แล้วดูหน่วยความจำของมัน
./mmap -p $$ --snapshot --no-color > snapshot.txt   # รายงานข้อความของ shell ปัจจุบัน
```

| ตัวเลือก | ความหมาย | ค่าเริ่มต้น |
|---|---|---|
| `-p PID`, `--pid PID` | โปรเซสที่ต้องการดู | (เปิดหน้าเลือกโปรเซส) |
| `-s`, `--snapshot` | พิมพ์รายงานครั้งเดียวแล้วจบ | ปิด |
| `--demo` | รันโปรเซสตัวอย่างแล้วดู (ใช้ร่วมกับ `-p` ไม่ได้) | ปิด |
| `-i MS`, `--interval MS` | รอบการอัปเดต 200–10000 มิลลิวินาที | 1000 |
| `-w N`, `--width N` | snapshot: ช่องของแผนที่ต่อบรรทัด (40–400) | 96 |
| `--no-color` | snapshot: ไม่ใส่สี ANSI (ปิดอัตโนมัติเมื่อ redirect/pipe หรือมี `NO_COLOR`) | ปิด |

ค่าที่ผิดรูปแบบหรือนอกช่วง และ PID ที่ไม่มีอยู่ จะถูกปฏิเสธพร้อม error และจบด้วย exit code 1

#### ปุ่มในหน้าจอเวลาจริง
| ปุ่ม | การทำงาน |
|---|---|
| `↑` `↓` / `j` `k` / ล้อเมาส์ | เลื่อนแถวที่เลือก |
| `PgUp` `PgDn` `g` `G` | เลื่อนทีละหน้า / ไปแถวแรก / แถวสุดท้าย |
| `Enter` หรือคลิกแถวซ้ำ | ดูทุกฟิลด์ของ region นั้นจาก smaps |
| คลิกบนแผนที่ | เลือก region ตรงตำแหน่งนั้น |
| `s` / `r` | เปลี่ยนคอลัมน์ที่เรียง / กลับลำดับ |
| `f` | กรองตามชนิดของ region |
| `t` หรือ `Tab` | สลับตาราง region กับตารางสรุปตามชนิด |
| `p` | เลือกโปรเซสอื่น |
| `space` / `u` | หยุด-เล่นต่อ / อัปเดตทันที |
| `+` / `-` | อัปเดตช้าลง / เร็วขึ้น |
| `?` | หน้าช่วยเหลือ |
| `q`, `Esc`, `Ctrl+C` | ออก |

### หน้าจอและโหมด
- **หน้าจอเวลาจริง:** memory (RSS เทียบ virtual, anon/file/shmem, swap, ค่าสูงสุด), rss history 60 วินาที,
  address space map (สีสด = อยู่ใน RAM, สีทึบ = map แล้วแต่ยังไม่อยู่ใน RAM, `┊` = ช่องว่าง, `▲` = region ที่เลือก),
  ตาราง regions และตารางสรุปตามชนิด (`t`)
- **แผนที่ถูกย่อสเกล:** ความกว้างของแต่ละ region เป็นสัดส่วนกับ log₂(จำนวนหน้า) + 1 และช่องว่างระหว่าง region ถูกย่อเหลือช่องเดียว
  ความกว้างบนแผนที่จึงไม่ใช่ขนาดจริง ให้ดูขนาดจริงจากตาราง
- **snapshot:** พิมพ์ 4 ส่วน คือค่าจาก `status`, สรุปตามชนิด (พร้อมบรรทัดตรวจ `sum of per-region Rss` เทียบ `VmRSS`),
  แผนที่ 3 บรรทัด (ตัวใหญ่ = อยู่ใน RAM, ตัวเล็ก = ไม่อยู่, `·` = ช่องว่าง) และตาราง region ทั้งหมด
- **`--demo`:** `mmap` รันตัวเองซ้ำเป็นโปรเซสลูก วนรอบละ ~21 วินาที (26 จังหวะ จังหวะละ 0.8 วินาที) ผ่านขั้น heap → bigvec → mmap → file → threads → stack → free
  โดยตั้งชื่อโปรเซสตามขั้นที่ทำอยู่ ใช้หน่วยความจำสูงสุด ~140 MiB

ชนิด region ที่แยกให้: `exe`, `heap`, `stack`, `thr-stack`, `lib`, `file`, `shm`, `anon`, `guard`, `kernel` (เงื่อนไขการแยกอยู่ใน `model.rs` และรายงาน)

### ข้อจำกัด
- ดูได้เฉพาะโปรเซสของผู้ใช้เอง (kernel จำกัดการอ่าน `maps`/`smaps`) ไม่ต้องและไม่ควรรันด้วย `sudo`
  โปรเซสของผู้ใช้อื่นจะเห็นเฉพาะค่าจาก `status` พร้อมคำเตือน
- ถ้าอ่าน `smaps` ไม่ได้แต่อ่าน `maps` ได้ จะแสดง region โดยไม่มี RSS/PSS
- ค่าจากแต่ละไฟล์ใน `/proc` อ่านไม่พร้อมกัน ผลรวม RSS ของทุก region จึงอาจต่างจาก `VmRSS` เล็กน้อย
- การจัดชนิด `thr-stack` เป็นการอนุมานจากตำแหน่ง อาจผิดได้บางกรณี
- kernel รวม region anonymous ที่ติดกันและสิทธิ์เหมือนกันเป็นหนึ่งเดียว (VMA merging)
- โปรเซสที่มี region จำนวนมาก (เช่นเบราว์เซอร์) อ่าน `smaps` ช้า ให้เพิ่มรอบอัปเดตด้วย `-i 2000`

### ความปลอดภัยต่อระบบ (DO NO HARM)
- อ่านไฟล์ใน `/proc` อย่างเดียว ไม่ใช้ `ptrace` ไม่เขียนหน่วยความจำของโปรเซสอื่น ไม่ต้องใช้ root และไม่เขียนไฟล์ลงดิสก์ (นอกจากที่ผู้ใช้ redirect เอง)
- คืนสภาพเทอร์มินัลทุกครั้งที่ออก รวมกรณี panic
- event loop รอ input ด้วย `poll` แบบมี timeout ไม่ busy loop
- โปรเซส demo ใช้หน่วยความจำไม่เกิน ~140 MiB, map ไฟล์ระบบแบบอ่านอย่างเดียว, ถูก kill เมื่อ `mmap` ออก (รวมกรณี panic),
  ตั้ง `PR_SET_PDEATHSIG` ไว้ และหยุดเองหลัง 30 นาทีเป็นอย่างช้า

---

## การใช้ AI
- เครื่องมือที่ใช้: Claude Code (Anthropic)
- ข้อ 1: `parallel.c`, `bench.sh`, `plot.py`, `Makefile`, `BUILD.md` เขียนโดยใช้ AI ช่วย
- ข้อ 2: ไฟล์ `.rs` ทั้งหมด, `Cargo.toml`, `Makefile`, `BUILD.md` เขียนโดยใช้ AI ช่วย (`Cargo.lock` สร้างอัตโนมัติโดย cargo)
- ไม่ได้ใช้ไฟล์ SKILL จึงไม่มีไฟล์ SKILL ในโฟลเดอร์ย่อย
- นิสิตเป็นผู้กำหนดการทดลองและความสามารถของโปรแกรม รัน benchmark และทดสอบบนเครื่องตัวเอง ตรวจโค้ดและผลลัพธ์ที่ AI เขียน
- รายงาน (`report_6710503755.pdf`): เนื้อหาและการวิเคราะห์เขียนโดยนิสิต ใช้ AI ตรวจคำผิด ช่วยสรุปผลการทดลอง และช่วยสร้างกราฟจากข้อมูลของข้อ 1
  รายละเอียดอยู่ในหัวข้อ "การใช้งาน AI" ของรายงาน
