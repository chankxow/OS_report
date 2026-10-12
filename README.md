# OS Report 2026

- ชื่อ-นามสกุล:เกียรติภูมิ พละสาร
- รหัสนิสิต: 6710503755

> โปรแกรมทั้งหมดในโฟลเดอร์นี้ทำขึ้นเพื่อการศึกษาในรายวิชาระบบปฏิบัติการเท่านั้น
> ไม่ใช่โปรแกรมสำหรับนำไปใช้งานจริง


## โครงสร้าง
```
os_report_6710503755/
├── 1_parallel_6710503755/   ข้อ 1: การคูณเมตริกซ์แบบขนานด้วย C + OpenMP
│   ├── parallel.c           โปรแกรมหลัก (ไฟล์เดียว)
│   ├── bench.sh             สคริปต์รันการทดลองทั้งหมด → CSV
│   ├── plot.py              สร้างกราฟและวิเคราะห์ Amdahl's law จาก CSV
│   ├── Makefile             `make` = คอมไพล์เป็น ./parallel
│   └── BUILD.md             วิธีคอมไพล์ รัน และทำการทดลองซ้ำ (ฉบับย่อ)
├── 2_mmap_6710503755/       ข้อ 2: เครื่องมือแสดงภาพ memory map ของโปรเซส (Rust)
│   ├── mmap.rs              โปรแกรมหลัก: อ่านตัวเลือก แล้วเลือกโหมด (live / snapshot / demo)
│   ├── procfs.rs            อ่านและแยกข้อมูลจาก /proc (อ่านอย่างเดียว)
│   ├── model.rs             ชนิดของ region, สรุปตามชนิด, การจัดวางแผนที่ address space
│   ├── ui.rs                หน้าจอแบบเต็มจอ (TUI) สไตล์ btop
│   ├── snapshot.rs          รายงานแบบข้อความครั้งเดียว
│   ├── demo.rs              โปรเซสตัวอย่างที่จอง/คืนหน่วยความจำเป็นจังหวะ
│   ├── Cargo.toml, Cargo.lock   รายการ dependency และเวอร์ชันที่ล็อกไว้
│   ├── Makefile             `make` = คอมไพล์เป็น ./mmap
│   └── BUILD.md             วิธีคอมไพล์ รัน และปุ่มควบคุม (ฉบับย่อ)
├── README.md
└── report_6710503755.pdf
```

---

## การใช้บนระบบปฏิบัติการอื่น (Windows / macOS)
ทั้งสองข้อพัฒนาและทดสอบบน Linux (Ubuntu 24.04) ผลการทดลองในรายงานวัดบน Linux ทั้งหมด

| | Linux | Windows 10/11 | macOS |
|---|---|---|---|
| ข้อ 1 (`parallel`) | `make` | ใช้ WSL2 แล้ว `make` เหมือน Linux | `brew install libomp` แล้ว `make` (ค่า Peak RSS/VmPeak ไม่ถูกต้อง) |
| ข้อ 2 (`mmap`) | `make` | ใช้ WSL2 แล้ว `make` เหมือน Linux | `make docker` (ต้องมี Docker Desktop) |

- **Windows:** ติดตั้ง WSL2 ด้วย `wsl --install` ใน PowerShell (Administrator) รีสตาร์ต แล้วเปิด Ubuntu และทำตามขั้นตอนของ Linux
  ข้อ 1 ใช้ `getrusage()` ที่ Windows ไม่มี และข้อ 2 อ่าน `/proc` ของ Linux จึงคอมไพล์ด้วย MinGW/MSVC โดยตรงไม่ได้
- **macOS ข้อ 1:** Apple clang ไม่มี OpenMP ในตัว `Makefile` จะใช้ `libomp` ของ Homebrew ให้เอง หรือใช้ `make CC=gcc-14` กับ gcc ของ Homebrew
  การคำนวณ เวลา และการตรวจคำตอบถูกต้อง แต่ `Peak RSS` แสดงสูงเกินจริง 1024 เท่า (`ru_maxrss` ของ macOS เป็น byte) และ `VmPeak` ใช้ไม่ได้ (ไม่มี `/proc`)
- **macOS ข้อ 2:** macOS ไม่มี `/proc` จึงรันใน Linux container แทน `make docker` คอมไพล์ด้วย `rust:1.75` แล้วเปิด `./mmap --demo`
  (เห็นเฉพาะโปรเซสใน container ไม่ใช่โปรแกรมของ macOS)
- ถ้าสั่ง `make` บนระบบที่ไม่รองรับ `Makefile` จะหยุดพร้อมบอกวิธีที่ถูกต้อง
- รายละเอียด ปัญหาที่พบบ่อย สิ่งที่ห้ามทำ และสิ่งที่ควรทำ อยู่ใน `BUILD.md` ของแต่ละข้อ

---

## ข้อ 1 — Intro to Parallel Programming (Matrix multiplication, C + OpenMP)

โปรแกรม `parallel.c` คำนวณ C = A × B สำหรับเมตริกซ์จัตุรัสขนาด n × n (ชนิด `double`) แบบขนานด้วย OpenMP
โดยจับเวลาแยกเป็นช่วง serial และ parallel เพื่อนำไปวิเคราะห์ด้วย Amdahl's law
ส่วน `bench.sh` ใช้รันการทดลองหลายมิติ (จำนวน thread, ขนาด n, การตั้งค่าโปรแกรม, การตั้งค่า OS, หน่วยความจำ)
และ `plot.py` ใช้สร้างกราฟกับตารางสรุปจากผลการทดลอง

### 1. สิ่งที่ต้องมี
| ใช้สำหรับ | สิ่งที่ต้องติดตั้ง |
|---|---|
| คอมไพล์และรันโปรแกรม | Linux, `gcc` ที่รองรับ OpenMP (`-fopenmp`) — ทดสอบกับ gcc 13.3 บน Ubuntu 24.04 (Windows/macOS ดูหัวข้อ "การใช้บนระบบปฏิบัติการอื่น") |
| รัน `bench.sh` | `bash`, `taskset` (util-linux), `/usr/bin/time` (แพ็กเกจ `time`), `timeout`, `nice`, `lscpu` |
| รัน `plot.py` | Python 3 + `matplotlib` (ไม่ใช้ไลบรารีภายนอกอื่น) |

บน Ubuntu ติดตั้งได้ด้วย
```sh
sudo apt install build-essential time python3-matplotlib
```

### 2. คอมไพล์
```sh
cd 1_parallel_6710503755
make            # Linux: gcc -O2 -Wall -Wextra -fopenmp -o parallel parallel.c -lm
make clean      # ลบ ./parallel
```
โปรแกรมหลักเป็น C ไฟล์เดียว แต่ในโฟลเดอร์มีไฟล์ประกอบอื่น (`bench.sh`, `plot.py`) จึงมี `Makefile` ให้ด้วย — คำสั่ง `make` อย่างเดียวพอ

### 3. รันโปรแกรม
```sh
./parallel -h                                   # แสดงตัวเลือกทั้งหมด
./parallel                                      # ค่าเริ่มต้น: n=800, ikj, ทุก CPU
./parallel -n 800 -t 8 -a ikj -v                # n=800, 8 threads, ตรวจความถูกต้องของคำตอบ
./parallel -n 1600 -t 4 -a blocked -b 64        # อัลกอริทึม blocked, tile 64×64
./parallel -n 400 -t 8 -s dynamic -c 4          # OpenMP schedule แบบ dynamic, chunk 4
./parallel -o csv -H -n 800 -t 8 -l mytest      # ผลลัพธ์เป็น CSV พร้อมบรรทัดหัวตาราง
```

#### ตัวเลือกทั้งหมด
| ตัวเลือก | ความหมาย | ค่าเริ่มต้น |
|---|---|---|
| `-n N` | ขนาดเมตริกซ์ n×n (1–4096) | 800 |
| `-t T` | จำนวน OpenMP threads (1 ถึง 4 × จำนวน CPU) | `OMP_NUM_THREADS` หรือจำนวน CPU |
| `-a ALGO` | อัลกอริทึม: `naive` \| `ikj` \| `transpose` \| `blocked` | `ikj` |
| `-b BS` | ขนาด block (tile BS×BS) ของ `blocked` | 64 |
| `-s SCHED` | OpenMP schedule: `static` \| `dynamic` \| `guided` | `static` |
| `-c CHUNK` | chunk size ของ schedule (0 = ค่า default ของ OpenMP) | 0 |
| `-r R` | ทำ kernel ซ้ำ R รอบ (1–100) แล้วรายงานเวลาดีที่สุดและเวลาเฉลี่ย | 3 |
| `-v` | ตรวจผลลัพธ์กับการคำนวณแบบ serial (n ≤ 800 ตรวจทุกช่อง, n > 800 สุ่มตรวจ 2,000 ช่อง, ยอมรับ relative error ≤ 1e-9) | ปิด |
| `-l LABEL` | ข้อความกำกับในคอลัมน์แรกของ CSV | `-` |
| `-o FORMAT` | รูปแบบผลลัพธ์: `table` \| `csv` | `table` ถ้าแสดงบนเทอร์มินัล, `csv` ถ้า redirect/pipe |
| `-H` | พิมพ์บรรทัดหัวตาราง CSV ก่อน (เฉพาะผลลัพธ์แบบ CSV) | ปิด |
| `-h` | แสดงวิธีใช้ | |

ค่าที่ไม่ถูกต้อง (เช่น `-n 5000`) จะถูกปฏิเสธพร้อมข้อความ error และโปรแกรมจบด้วย exit code 1
ถ้าใช้ `-v` แล้วผลตรวจไม่ผ่าน โปรแกรมจะจบด้วย exit code 1 เช่นกัน

#### ตั้งค่าผ่าน environment variable ของ OpenMP / OS
โปรแกรมใช้ `schedule(runtime)` และ OpenMP runtime ปกติ จึงปรับพฤติกรรมจากภายนอกได้ เช่น
```sh
OMP_PROC_BIND=close OMP_PLACES=cores ./parallel -n 1600 -t 8   # ผูก thread กับ core
OMP_WAIT_POLICY=passive ./parallel -n 400                       # thread ที่รอจะหลับแทนการ spin
OMP_STACKSIZE=64M ./parallel -n 100 -t 16                       # ขนาด stack ต่อ thread
taskset -c 0-3 ./parallel -n 1600 -t 8                          # จำกัดให้ใช้ได้แค่ CPU 0–3
nice -n 19 ./parallel -n 1600                                   # ลดลำดับความสำคัญของโปรเซส
```

### 4. อัลกอริทึม
ทุกอัลกอริทึมแบ่งงานแบบขนานที่ลูปนอกสุด (แถวของ C หรือกลุ่มแถวของ C) แต่ละช่องของ C จึงถูกเขียนโดย
thread เดียวเท่านั้น ไม่ต้องใช้ lock หรือ atomic

| `-a` | วิธีการ | จุดเด่น / ข้อสังเกต |
|---|---|---|
| `naive` | ลูป i-j-k ตามนิยาม | ลูปในสุดอ่าน B ลงตามคอลัมน์ (stride n) ใช้ cache ได้ไม่ดี |
| `ikj` | สลับลูปเป็น i-k-j | ลูปในสุดอ่าน B และเขียน C ตามแถว (stride 1) เป็นมิตรกับ cache และ vectorize ได้ |
| `transpose` | transpose B เป็น Bᵀ ก่อน แล้วคิดแต่ละช่องเป็น dot product ของสองแถว | ใช้หน่วยความจำเพิ่มอีก n² (รวม 4 เมตริกซ์) |
| `blocked` | i-k-j แบบแบ่ง tile ขนาด BS×BS | ให้ข้อมูลที่ใช้อยู่ค้างใน cache; ขนานตามกลุ่มแถวของ C |

เมตริกซ์ A และ B สร้างจาก pseudo-random generator (LCG) ที่มี seed คงที่ ทุกการรันจึงได้ข้อมูลเข้าชุดเดียวกัน
และ checksum เท่ากันทุกครั้ง ใช้เทียบความถูกต้องข้ามอัลกอริทึมและจำนวน thread ได้

### 5. ผลลัพธ์ของโปรแกรม
โปรแกรมจับเวลาแต่ละช่วงด้วย `omp_get_wtime()` แยกกัน

| ช่วง | ประเภท | สิ่งที่ทำ |
|---|---|---|
| `t_init` | serial | `malloc` และเติมค่า A, B |
| `t_mul_best` / `t_mul_mean` | **parallel** | kernel คูณเมตริกซ์ (ดีที่สุด / เฉลี่ยจาก `-r` รอบ) |
| `t_post` | serial | คำนวณ checksum ของ C |
| `t_verify` | serial | ตรวจคำตอบ (เฉพาะเมื่อใช้ `-v`) |

**แบบตาราง** (เมื่อรันบนเทอร์มินัล) แสดง 3 ส่วน: Configuration, Timing (เวลาและสัดส่วน % ของแต่ละช่วง)
และ Results (GFLOP/s, หน่วยความจำ, checksum, ผลการตรวจ) ตัวอย่างส่วน Timing:
```
 Timing
┌──────────────────────┬──────────┬──────────┬─────────┐
│ Phase                │ Type     │ Time (s) │   Share │
├──────────────────────┼──────────┼──────────┼─────────┤
│ init (malloc + fill) │ serial   │ 0.000357 │  13.6 % │
│ multiply (best)      │ parallel │ 0.002232 │  85.2 % │
│ multiply (mean)      │ parallel │ 0.002475 │       - │
│ checksum             │ serial   │ 0.000031 │   1.2 % │
├──────────────────────┼──────────┼──────────┼─────────┤
│ total                │          │ 0.002619 │ 100.0 % │
└──────────────────────┴──────────┴──────────┴─────────┘
```

**แบบ CSV** (เมื่อ redirect/pipe หรือใช้ `-o csv`) พิมพ์ 1 บรรทัดต่อการรัน 1 ครั้ง เพื่อให้สคริปต์ต่อท้ายไฟล์ได้
```
label,algo,n,threads,schedule,chunk,block,t_init,t_mul_best,t_mul_mean,t_post,t_verify,t_total,gflops,mem_theory_mb,maxrss_mb,vmpeak_mb,checksum,max_rel_err,verified
```
| คอลัมน์ | ความหมาย |
|---|---|
| `threads` | จำนวน thread ที่ได้จริง (อาจน้อยกว่าที่ขอ) |
| `t_total` | `t_init + t_mul_best + t_post + t_verify` (วินาที) |
| `gflops` | 2n³ / `t_mul_best` / 10⁹ |
| `mem_theory_mb` | ขนาดเมตริกซ์ตามทฤษฎี (3 หรือ 4 เมตริกซ์ × n² × 8 ไบต์) |
| `maxrss_mb` | หน่วยความจำกายภาพสูงสุด (peak RSS จาก `getrusage`) |
| `vmpeak_mb` | หน่วยความจำเสมือนสูงสุด (`VmPeak` จาก `/proc/self/status`) รวม stack ที่จองไว้ของทุก thread |
| `verified` | 1 = ถูกต้อง, 0 = ผิด, -1 = ไม่ได้ตรวจ |

### 6. ทำการทดลองทั้งหมดซ้ำ
```sh
cd 1_parallel_6710503755
./bench.sh ~/bench_results quick    # รุ่นย่อ ~15 วินาที ไว้ทดสอบว่าสคริปต์ทำงานได้
./bench.sh ~/bench_results          # การทดลองเต็ม ~45–60 นาที
python3 plot.py ~/bench_results     # สร้างกราฟและ summary.md
```
- อาร์กิวเมนต์แรกคือโฟลเดอร์ผลลัพธ์ (ถ้าไม่ระบุจะใช้ `./results` ข้างซอร์สโค้ด) — แนะนำให้ระบุโฟลเดอร์นอก
  `os_report_6710503755/` เพื่อไม่ให้มีไฟล์เกินในโฟลเดอร์ส่งงาน
- `bench.sh` คอมไพล์โปรแกรมเองลงใน `<OUT>/bin/` ทั้งแบบ `-O2`, `-O0` และ `-O3 -march=native`
- ระหว่างการทดลองควรปิดโปรแกรมอื่นและเสียบสายชาร์จ เพื่อลดสัญญาณรบกวนในการวัด

#### การทดลองที่ `bench.sh` รัน (การทดลองเต็ม)
ค่า `NCORE` = จำนวน physical core, `NCPU` = จำนวน logical CPU (รวม SMT) ของเครื่องที่รัน

| ไฟล์ใน `<OUT>/raw/` | สิ่งที่เปลี่ยน | หัวข้อในโจทย์ |
|---|---|---|
| `sysinfo.txt` | ข้อมูลเครื่อง: CPU, gcc, governor, SMT, THP, `ulimit -s`, load average, หน่วยความจำ | — |
| `scaling.csv` | threads 1–16 × n = 100–3200 × อัลกอริทึม naive/ikj/blocked | 2.1, 2.2 |
| `algos.csv` | 4 อัลกอริทึม × n × threads {1, NCORE, NCPU} | 2.2, 2.3 |
| `sched.csv` | schedule static/dynamic/guided × chunk {0, 1, 4, 16, 64} | 2.3 |
| `block.csv` | block size 8–512 ของ `blocked` | 2.3 |
| `flags.csv` | ระดับ optimization ของคอมไพเลอร์ `-O0` / `-O2` / `-O3 -march=native` | 2.3 |
| `bind.csv` | `OMP_PROC_BIND` (false/close/spread) × `OMP_PLACES` (threads/cores) | 2.4 |
| `affinity.csv` | CPU affinity mask ด้วย `taskset` และกรณี thread มากกว่า CPU ที่อนุญาต (oversubscription) | 2.4 |
| `wait.csv` | `OMP_WAIT_POLICY` active / passive | 2.4 |
| `nice.csv` | `nice` 0 / 19 โดยมีและไม่มีโหลด CPU อื่นแข่ง | 2.4 |
| `memory.csv` | หน่วยความจำตาม n และ threads + ตรวจไขว้กับ `/usr/bin/time` | 2.5 |
| `stack.csv` | หน่วยความจำต่อ thread ตาม `OMP_STACKSIZE` (default / 1M / 64M) | 2.5 |

จำนวนรอบที่ทำ kernel ซ้ำปรับตาม n (n ≤ 400: 10 รอบ, ≤ 800: 5, ≤ 1600: 3, มากกว่านั้น: 2)
เพื่อลดสัญญาณรบกวนของการรันที่สั้น

#### กราฟที่ `plot.py` สร้าง (`<OUT>/figures/`)
| ไฟล์ | เนื้อหา |
|---|---|
| `fig1_time_vs_n.png` | เวลา kernel เทียบกับ n (4 อัลกอริทึม) |
| `fig1b_time_vs_threads.png` | เวลา kernel เทียบกับจำนวน thread |
| `fig2_speedup.png` | speedup T(1)/T(p) แยกตาม n |
| `fig3_efficiency.png` | efficiency S(p)/p |
| `fig4_gflops_vs_n.png` | GFLOP/s เทียบกับ n |
| `fig5_schedule.png` | OpenMP schedule × chunk size |
| `fig6_block_size.png` | ผลของ block size ต่ออัลกอริทึม blocked |
| `fig7_compiler_flags.png` | ผลของระดับ optimization ของคอมไพเลอร์ |
| `fig8_binding.png` | การวาง thread (`OMP_PROC_BIND` × `OMP_PLACES`) |
| `fig9_affinity_nice.png` | CPU affinity mask และ `nice` ภายใต้โหลดแข่ง |
| `fig11_memory.png` | หน่วยความจำเทียบกับ n และ overhead ต่อ thread (virtual vs physical) |
| `fig12_amdahl.png` | fit Amdahl's law และ Karp–Flatt serial fraction e(p) |

นอกจากนี้ยังเขียน `<OUT>/summary.md` ซึ่งเป็นตารางตัวเลขสรุป (สัดส่วน parallel ที่ fit ได้, speedup,
เวลาแต่ละช่วงที่ 1 thread, GFLOP/s ของแต่ละอัลกอริทึม ฯลฯ) สำหรับใช้อ้างอิงในรายงาน

### 7. การวิเคราะห์ parallel vs serial (Amdahl's law)
`plot.py` ประมาณสัดส่วนส่วนที่ทำขนานได้ f จากข้อมูลที่วัดจริงใน `scaling.csv` 3 วิธี
- **f_kernel** — fit แบบ least squares ของ S(p) = 1 / ((1 − f) + f/p) กับ speedup ของ kernel อย่างเดียว
- **f_program** — fit แบบเดียวกันกับเวลาทั้งโปรแกรม (init + kernel 1 รอบ + checksum)
- **f_phase** — สัดส่วนเวลาช่วง parallel ต่อเวลาทั้งหมดเมื่อรัน 1 thread (วัดตรงจาก `t_mul / t_total`)

การ fit ใช้เฉพาะ 2 ≤ p ≤ จำนวน physical core เพราะ thread ที่เกินจากนั้นใช้ SMT ซึ่งไม่ได้เพิ่มหน่วยประมวลผลจริง
และรายงาน Karp–Flatt serial fraction e(p) กับขีดจำกัดของ speedup S_max = 1 / (1 − f) ประกอบด้วย

### 8. ความปลอดภัยต่อระบบ (DO NO HARM)
- จำกัด n ≤ 4096 (3 เมตริกซ์ ≈ 384 MiB) และจำกัดจำนวน thread ≤ 4 เท่าของจำนวน CPU
- ตรวจผลของ `malloc` ทุกครั้ง และ `free` หน่วยความจำก่อนจบโปรแกรม
- ตรวจค่า argument ทุกตัว ค่าที่ผิดรูปแบบหรืออยู่นอกช่วงจะถูกปฏิเสธ
- `bench.sh` ไม่ต้องใช้สิทธิ์ root และไม่แก้ไขการตั้งค่าใดๆ ของระบบ (CPU governor ฯลฯ ถูก *บันทึก* ใน `sysinfo.txt` เท่านั้น)
- โหลด CPU ที่ใช้ในการทดลอง `nice` ถูกจำกัดเวลาด้วย `timeout 600` และถูก kill อัตโนมัติเมื่อสคริปต์จบหรือถูกกด Ctrl+C

---

## ข้อ 2 — Memory Map Visualization (Rust)

โปรแกรม `mmap` แสดงภาพการใช้หน่วยความจำของโปรเซสบน Linux โดยอ่านข้อมูล memory map จากระบบปฏิบัติการโดยตรง
(`/proc/PID/maps`, `/proc/PID/smaps`, `/proc/PID/status`, `/proc/meminfo`) แสดงผลได้ 2 แบบ
- **แบบเวลาจริง (real time)** — หน้าจอเต็มจอสไตล์ btop อัปเดตทุก 1 วินาที (ปรับได้) ใช้คีย์บอร์ดและเมาส์
- **แบบ snapshot** — พิมพ์รายงานข้อความครั้งเดียวแล้วจบ ใช้ redirect ลงไฟล์หรือแคปใส่รายงานได้

### 1. สิ่งที่ต้องมี
| ใช้สำหรับ | สิ่งที่ต้องติดตั้ง |
|---|---|
| คอมไพล์ | Linux, `make`, Rust toolchain (`cargo` + `rustc` ≥ 1.75) (Windows/macOS ดูหัวข้อ "การใช้บนระบบปฏิบัติการอื่น") |
| ดาวน์โหลด dependency | อินเทอร์เน็ต (เฉพาะการคอมไพล์ครั้งแรก — cargo ดาวน์โหลด crate `ratatui` และ `libc` ตามเวอร์ชันใน `Cargo.lock`) |
| รันโหมดเวลาจริง | เทอร์มินัลขนาดอย่างน้อย 70 × 22 ตัวอักษร ที่รองรับสี 24-bit (เช่น GNOME Terminal, Konsole, kitty, VS Code) |

บน Ubuntu ติดตั้งได้ด้วย
```sh
sudo apt install build-essential cargo
```
หรือติดตั้ง Rust เวอร์ชันล่าสุดผ่าน https://rustup.rs

`Cargo.lock` ล็อกเวอร์ชันของ crate ทุกตัวให้คอมไพล์ได้กับ Rust 1.75 (เวอร์ชันใน apt ของ Ubuntu 24.04) — ทดสอบแล้วบน Ubuntu 24.04, Linux 7.0, rustc 1.75.0

### 2. คอมไพล์
```sh
cd 2_mmap_6710503755
make            # = cargo build --release --locked แล้วคัดลอกไฟล์ที่ได้มาเป็น ./mmap
make test       # รัน unit test (การ parse ไฟล์ /proc, การจัดประเภท region, การวางแผนที่)
make clean      # ลบ target/ และ ./mmap
```
ไม่ใช่ C/C++/Python และมีหลายไฟล์ จึงมี `Makefile` ให้ — คำสั่ง `make` อย่างเดียวพอ

### 3. รันโปรแกรม
```sh
./mmap -h                          # แสดงตัวเลือกทั้งหมด
./mmap                             # เปิดหน้าเลือกโปรเซส แล้วเข้าสู่หน้าจอเวลาจริง
./mmap -p 1234                     # ดูโปรเซส PID 1234 แบบเวลาจริง
./mmap --demo                      # รันโปรเซสตัวอย่าง แล้วดูหน่วยความจำของมันเพิ่ม/ลด
./mmap -p $$ --snapshot            # รายงานแบบข้อความของ shell ปัจจุบัน
./mmap -p $$ --snapshot --no-color > snapshot.txt   # บันทึกเป็นไฟล์ข้อความล้วน
./mmap --demo --snapshot           # รอให้ demo จองหน่วยความจำ ~6 วินาที แล้วพิมพ์รายงาน
```

#### ตัวเลือกทั้งหมด
| ตัวเลือก | ความหมาย | ค่าเริ่มต้น |
|---|---|---|
| `-p PID`, `--pid PID` | โปรเซสที่ต้องการดู | (เปิดหน้าเลือกโปรเซส) |
| `-s`, `--snapshot` | พิมพ์รายงานข้อความครั้งเดียวแล้วจบ | ปิด |
| `--demo` | รันโปรเซสตัวอย่างแล้วดูโปรเซสนั้น (ใช้ร่วมกับ `-p` ไม่ได้) | ปิด |
| `-i MS`, `--interval MS` | รอบการอัปเดตของหน้าจอเวลาจริง หน่วยมิลลิวินาที (200–10000) | 1000 |
| `-w N`, `--width N` | snapshot: จำนวนช่องของแผนที่ต่อบรรทัด (40–400) | 96 |
| `--no-color` | snapshot: ไม่ใส่สี ANSI (ปิดสีอัตโนมัติเมื่อ redirect/pipe หรือมี `NO_COLOR`) | ปิด |
| `-h`, `--help` | แสดงวิธีใช้ | |

ค่าที่ผิดรูปแบบหรืออยู่นอกช่วง และ PID ที่ไม่มีอยู่ จะถูกปฏิเสธพร้อมข้อความ error และจบด้วย exit code 1

#### ปุ่มในหน้าจอเวลาจริง
| ปุ่ม | การทำงาน |
|---|---|
| `↑` `↓` / `j` `k` / ล้อเมาส์ | เลื่อนแถวที่เลือกในตาราง |
| `PgUp` `PgDn` `g` `G` | เลื่อนทีละหน้า / ไปแถวแรก / แถวสุดท้าย |
| `Enter` หรือคลิกแถวที่เลือกซ้ำ | ดูรายละเอียดทุกฟิลด์ของ region นั้นจาก smaps |
| คลิกบนแผนที่ | เลือก region ที่อยู่ตรงตำแหน่งนั้น |
| `s` / `r` | เปลี่ยนคอลัมน์ที่ใช้เรียง (address, size, RSS, PSS, dirty, swap) / กลับลำดับ |
| `f` | กรองตามชนิดของ region (กดวนไปเรื่อยๆ จนกลับมาแสดงทั้งหมด) |
| `t` หรือ `Tab` | สลับระหว่างตาราง region กับตารางสรุปตามชนิด |
| `p` | เลือกโปรเซสอื่น (พิมพ์เพื่อค้นหาจาก PID ชื่อ หรือคำสั่ง) |
| `space` | หยุด / เล่นต่อ การอัปเดต |
| `+` / `-` | อัปเดตช้าลง / เร็วขึ้น |
| `u` | อัปเดตทันที |
| `?` | แสดงหน้าช่วยเหลือ |
| `q`, `Esc`, `Ctrl+C` | ออกจากโปรแกรม |

### 4. หน้าจอเวลาจริงประกอบด้วย
| ส่วน | แสดงอะไร | ที่มาของข้อมูล |
|---|---|---|
| memory | RSS เทียบกับขนาด virtual, RSS แยกเป็น anon / file / shmem, swap, หน่วยความจำทั้งเครื่อง, ค่าสูงสุด (VmHWM, VmPeak), จำนวน thread และจำนวน region | `/proc/PID/status`, `/proc/meminfo` |
| rss history | กราฟ RSS, anon และ file ย้อนหลัง 60 วินาที | `/proc/PID/status` ทุกรอบการอัปเดต |
| address space | แผนที่ region ทั้งหมดเรียงจากที่อยู่ต่ำไปสูง สีตามชนิด สีสด = ส่วนที่อยู่ใน RAM สีทึบ = map ไว้แต่ยังไม่อยู่ใน RAM `┊` = ช่องว่างที่ไม่ได้ map และ `▲` ชี้ region ที่เลือก | `/proc/PID/smaps` |
| regions | ตาราง region: ที่อยู่เริ่ม–จบ, ขนาด, สิทธิ์, RSS, % ที่อยู่ใน RAM, PSS, dirty, swap, ชนิด และ path | `/proc/PID/maps` + `smaps` |
| by kind (`t`) | ผลรวมขนาด/RSS/PSS/swap/dirty ของแต่ละชนิด และสัดส่วนต่อ RSS ทั้งหมด | คำนวณจาก `smaps` |

**การย่อสเกลของแผนที่:** address space ของโปรเซส 64-bit กว้างถึง 128 TiB แต่ใช้จริงเพียงไม่กี่ MiB ถ้าวาดตามสัดส่วนจริงจะมองไม่เห็นอะไรเลย
แผนที่จึงให้ความกว้างของแต่ละ region เป็นสัดส่วนกับ log₂(จำนวนหน้า) + 1 และย่อช่องว่างระหว่าง region ให้เหลือช่องเดียว —
**ความกว้างบนแผนที่จึงไม่ใช่ขนาดจริง** ให้ดูขนาดจริงจากตาราง

#### ชนิดของ region ที่โปรแกรมแยกให้
| ชนิด | เงื่อนไขที่ใช้แยก | ความหมาย |
|---|---|---|
| `exe` | path ตรงกับ `/proc/PID/exe` | โค้ดและข้อมูลของตัวโปรแกรมเอง (text, data, bss) |
| `heap` | `[heap]` | พื้นที่ malloc ที่ขยายด้วย `brk()` |
| `stack` | `[stack]` | stack ของ main thread ขยายลงเมื่อถูกใช้ |
| `thr-stack` | anonymous ขนาด ≥ 16 KiB ที่ต่อจาก guard page ขนาด ≤ 64 KiB | stack ของ thread อื่น (เป็นการอนุมาน) |
| `lib` | ชื่อไฟล์ลงท้าย `.so` หรือมี `.so.` | shared library |
| `file` | มี path อื่นๆ | ไฟล์ที่ถูก `mmap()` |
| `shm` | `/dev/shm/`, `memfd:`, `SYSV`, anonymous ที่เป็น shared | หน่วยความจำที่ใช้ร่วมกันระหว่างโปรเซส |
| `anon` | ไม่มี path | หน่วยความจำ anonymous (malloc ก้อนใหญ่, `mmap()`) |
| `guard` | สิทธิ์ `---p` ไม่มี path | หน้าที่ห้ามเข้าถึง: guard page หรือพื้นที่ที่จองไว้ |
| `kernel` | `[vdso]`, `[vvar]`, `[vsyscall]` | หน้าที่ kernel map ให้ทุกโปรเซส |

### 5. โหมด snapshot
พิมพ์ 4 ส่วนตามลำดับ
1. ข้อมูลโปรเซสและค่าจาก `/proc/PID/status` (VmSize, VmPeak, VmRSS, VmHWM, RssAnon, RssFile, RssShmem, VmSwap, VmData, VmStk, VmExe, VmLib)
2. สรุปตามชนิดของ region พร้อมบรรทัดตรวจสอบ `sum of per-region Rss` เทียบกับ `VmRSS`
3. แผนที่ address space 3 บรรทัด แต่ละบรรทัดขึ้นต้นด้วยที่อยู่ของ region แรกในบรรทัดนั้น
   ใช้อักษรแทนชนิด (`E` exe, `H` heap, `S` stack, `T` thread stack, `L` lib, `F` file, `M` shm, `A` anon, `G` guard, `K` kernel)
   **ตัวใหญ่** = ส่วนที่อยู่ใน RAM, **ตัวเล็ก** = map ไว้แต่ไม่อยู่ใน RAM, `·` = ช่องว่าง
4. ตาราง region ทั้งหมด (START, END, SIZE, PERM, RSS, PSS, DIRTY, SWAP, KIND, PATH)

### 6. โปรเซสตัวอย่าง (`--demo`)
`mmap` รันตัวเองซ้ำเป็นโปรเซสลูก ซึ่งทำงานเป็นรอบ รอบละประมาณ 20 วินาที (ขั้นละ 0.8 วินาที) และตั้งชื่อโปรเซส (`/proc/PID/comm`) ตามขั้นที่กำลังทำ
จึงเห็นชื่อขั้นที่มุมบนซ้ายของหน้าจอ

| ขั้น (ชื่อโปรเซส) | สิ่งที่ทำ | สิ่งที่ควรเห็น |
|---|---|---|
| `demo:heap` | จองก้อนละ 16 KiB รวม 200 ก้อน (~3 MiB) | `[heap]` ขยายใหญ่ขึ้น (glibc ใช้ `brk()` กับคำขอ < 128 KiB) |
| `demo:bigvec` | จอง 2 ก้อน ก้อนละ 32 MiB | region `anon` ใหม่ 2 อัน (glibc ใช้ `mmap()` กับคำขอ ≥ 128 KiB) |
| `demo:mmap` | `mmap()` 64 MiB แล้วเขียนทีละ 8 MiB | VmSize เพิ่มทันที 64 MiB แต่ RSS ค่อยๆ เพิ่ม (demand paging) |
| `demo:file` | `mmap()` ไฟล์ระบบแบบอ่านอย่างเดียว (เช่น `/etc/ssl/certs/ca-certificates.crt`) | region `file` ใหม่ RSS นับเป็น RssFile |
| `demo:threads` | สร้าง thread 2 ตัว stack ตัวละ 1 MiB | คู่ `guard` + `thr-stack` 2 คู่ จำนวน thread เป็น 3 |
| `demo:stack` | เรียกฟังก์ชันซ้อน 25 ชั้น ชั้นละ 64 KiB | `[stack]` ขยาย ~1.6 MiB และไม่หดกลับแม้ฟังก์ชันคืนค่าแล้ว |
| `demo:free` | คืนหน่วยความจำทั้งหมดทีละส่วน และเรียก `malloc_trim()` | region หายไป RSS ลดลง |

ใช้หน่วยความจำสูงสุดประมาณ 140 MiB

### 7. ข้อจำกัดและสิ่งที่ควรรู้
- **ดูได้เฉพาะโปรเซสของผู้ใช้เอง** — kernel อนุญาตให้อ่าน `maps`/`smaps` ของโปรเซสอื่นเฉพาะเจ้าของหรือ root
  ในหน้าเลือกโปรเซส แถวของผู้ใช้อื่นจะเป็นสีจาง ถ้าเลือก จะเห็นเฉพาะค่าจาก `status` พร้อมข้อความเตือน (ไม่ต้องและไม่ควรรันด้วย `sudo`)
- ถ้าอ่าน `smaps` ไม่ได้แต่อ่าน `maps` ได้ โปรแกรมจะแสดงรายการ region โดยไม่มีค่า RSS/PSS (แสดงเป็น `-`)
- **kernel thread** (เช่น `kthreadd`) ไม่มีหน่วยความจำฝั่ง user โปรแกรมจะแจ้งว่าไม่มี mapping
- ถ้าโปรเซสที่ดูอยู่จบการทำงาน หน้าจอจะคงข้อมูลล่าสุดไว้และแจ้งให้กด `p` เพื่อเลือกโปรเซสอื่น
- ค่าที่อ่านแต่ละไฟล์ใน `/proc` ไม่ได้อ่านพร้อมกันในเสี้ยววินาทีเดียว ผลรวม RSS ของทุก region จึงอาจต่างจาก `VmRSS` เล็กน้อย
  และ `VmRSS` ของ kernel เป็นตัวนับที่อัปเดตแบบแบ่งก้อน (per-CPU batching) จึงอาจคลาดได้อีกเล็กน้อย
- การอ่าน `smaps` ของโปรเซสที่มี region จำนวนมาก (เช่นเบราว์เซอร์) ใช้เวลานานขึ้น ถ้าหน้าจอช้า ให้เพิ่มรอบการอัปเดตด้วย `+` หรือ `-i 2000`
- การจัดชนิด `thr-stack` เป็นการอนุมานจากตำแหน่งที่ติดกับ guard page จึงอาจผิดได้ในบางกรณี
  ตัวอย่างที่พบ: region ขนาด 8 KiB ที่ต่อจาก guard page คือ alternate signal stack ที่ Rust สร้างไว้จัดการ stack overflow
  ไม่ใช่ stack ของ thread (จึงจัดเป็น `anon`)
- region `guard` ขนาดใหญ่ (เช่นเกือบ 64 MiB) ไม่ใช่ guard page แต่เป็นพื้นที่ address ที่ glibc จองไว้ให้ malloc arena ของ thread
- kernel รวม region anonymous ที่อยู่ติดกันและมีสิทธิ์เหมือนกันเป็น region เดียว (VMA merging)
  เช่น `Vec` ขนาด 32 MiB สองก้อนใน demo อาจแสดงเป็น region เดียวขนาด 64 MiB
- PSS = RSS ที่หารส่วนแบ่งตามจำนวนโปรเซสที่ใช้หน้านั้นร่วมกัน จึงเหมาะกับการบอกว่าโปรเซสนี้ "ใช้ RAM จริง" เท่าไร

### 8. ความปลอดภัยต่อระบบ (DO NO HARM)
- อ่านไฟล์ใน `/proc` อย่างเดียว ไม่ใช้ `ptrace` ไม่อ่านหรือเขียนหน่วยความจำของโปรเซสอื่น และไม่ต้องใช้สิทธิ์ root
- ไม่เขียนไฟล์ใดๆ ลงดิสก์ (นอกจากผู้ใช้ redirect ผลลัพธ์เอง)
- คืนสภาพเทอร์มินัล (ออกจาก alternate screen, ปิด raw mode และ mouse capture) ทุกครั้งที่ออก รวมถึงกรณีโปรแกรม panic
- event loop รอ input ด้วย `poll` แบบมี timeout ไม่วนตลอดเวลา จึงใช้ CPU น้อย
- โปรเซส demo ใช้หน่วยความจำไม่เกิน ~140 MiB, map ไฟล์ระบบแบบอ่านอย่างเดียว, ถูก kill ทันทีเมื่อ `mmap` ออก
  (รวมกรณี panic) และตั้ง `PR_SET_PDEATHSIG` ไว้ จึงจบเองถ้าโปรเซสแม่ตายกะทันหัน และหยุดเองหลัง 30 นาทีเป็นอย่างช้า
- ตรวจสอบค่าอาร์กิวเมนต์ทุกตัว ค่าที่ผิดรูปแบบหรืออยู่นอกช่วงจะถูกปฏิเสธ

---

## การใช้ AI
- เครื่องมือที่ใช้: Claude Code (Anthropic)
- ข้อ 1: `parallel.c`, `bench.sh`, `plot.py`, `Makefile`, `BUILD.md` เขียนโดยใช้ AI ช่วย
- ข้อ 2: ไฟล์ `.rs` ทั้งหมด, `Cargo.toml`, `Makefile`, `BUILD.md` เขียนโดยใช้ AI ช่วย (`Cargo.lock` สร้างอัตโนมัติโดย cargo)
- ไม่ได้ใช้ไฟล์ SKILL จึงไม่มีไฟล์ SKILL ในโฟลเดอร์ย่อย
- รายงาน (`report_6710503755.pdf`) เขียนเองทั้งหมดแต่มีการใช้ AI ในการเรียบเรียงหัวข้อและจัดองค์ประกอบ และมีหัวข้ออธิบายการใช้ AI ของทั้งสองข้อโดยละเอียด
# OS_report
