# BUILD — 1_parallel (Matrix multiplication with C + OpenMP)

## สิ่งที่ต้องมี
- Linux, `gcc` ที่รองรับ OpenMP (ทดสอบกับ gcc 13.3, Ubuntu 24.04)
- สำหรับรัน benchmark: `bash`, `taskset` (util-linux), `/usr/bin/time` (แพ็กเกจ `time`)
- สำหรับพล็อตกราฟ: Python 3 + `matplotlib`

## คอมไพล์
```sh
make            # = gcc -O2 -Wall -Wextra -fopenmp -o parallel parallel.c -lm
```
`Makefile` ตรวจระบบปฏิบัติการเองแล้วเลือกคอมไพเลอร์และ flag ของ OpenMP ให้

## ระบบปฏิบัติการอื่น

| ระบบ | วิธี | สถานะ |
|---|---|---|
| Linux | `make` | ทดสอบแล้ว (ใช้กับผลการทดลองทั้งหมดในรายงาน) |
| Windows 10/11 | รันใน WSL2 (Ubuntu) ด้วยขั้นตอนเดียวกับ Linux | ใช้ได้เหมือน Linux ทุกอย่าง |
| macOS | `make` (Apple clang + Homebrew `libomp`) | คอมไพล์และรันได้ แต่ค่าหน่วยความจำบางค่าไม่ถูกต้อง (ดูด้านล่าง) |

**Windows: ใช้ WSL2**

โค้ดใช้ `getrusage()` จาก `<sys/resource.h>` ซึ่งไม่มีบน Windows จึงคอมไพล์ตรงๆ ด้วย MinGW/MSVC ไม่ได้
(`make` บน Windows จะหยุดพร้อมแจ้งให้ใช้ WSL2) ให้ติดตั้ง WSL2 ซึ่งเป็น Linux จริงที่รันบน Windows
```powershell
wsl --install            # ใน PowerShell (Administrator) แล้วรีสตาร์ตเครื่อง
```
จากนั้นเปิด Ubuntu แล้วทำตามขั้นตอนของ Linux
```sh
sudo apt update && sudo apt install build-essential
cd /mnt/c/Users/<ชื่อผู้ใช้>/Downloads/os_report_6710503755/1_parallel_6710503755   # ไดรฟ์ C: อยู่ที่ /mnt/c
make
./parallel -n 800 -t 8 -a ikj -v
```

**macOS: Apple clang + libomp**

Apple clang ไม่มี OpenMP runtime ในตัว ต้องติดตั้ง `libomp` ผ่าน [Homebrew](https://brew.sh) ก่อน
```sh
xcode-select --install   # ติดตั้งคอมไพเลอร์ (ครั้งแรกครั้งเดียว)
brew install libomp
make                     # = clang -O2 -Wall -Wextra -Xpreprocessor -fopenmp -I$(brew --prefix libomp)/include ... -lomp -lm
```
ถ้าต้องการใช้ gcc ของ Homebrew แทน: `brew install gcc` แล้ว `make CC=gcc-14` (ไม่ต้องใช้ libomp)
(เปลี่ยน `14` ตามเวอร์ชันที่ติดตั้ง)

ข้อจำกัดบน macOS: การคูณเมตริกซ์ เวลาแต่ละช่วง GFLOP/s และการตรวจคำตอบ (`-v`) ถูกต้องตามปกติ แต่
- `Peak RSS` / `maxrss_mb` แสดงค่าสูงเกินจริง 1024 เท่า เพราะ `ru_maxrss` ของ macOS มีหน่วยเป็น byte ส่วนโค้ดถือว่าเป็น KiB ตามแบบ Linux
- `Peak virtual (VmPeak)` / `vmpeak_mb` เป็น `-1` เพราะ macOS ไม่มี `/proc/self/status`

**สคริปต์ทดลอง:** `bench.sh` ใช้เครื่องมือเฉพาะ Linux (`taskset`, `lscpu`, `nproc`, `/proc/loadavg`, GNU `/usr/bin/time`)
จึงรันได้บน Linux และ WSL2 เท่านั้น ส่วน `plot.py` รันได้ทุกระบบที่มี Python 3 + `matplotlib`

## รัน
```sh
./parallel -h                          # ดูตัวเลือกทั้งหมด
./parallel -n 800 -t 8 -a ikj -v       # n=800, 8 threads, อัลกอริทึม ikj, ตรวจคำตอบ (แสดงเป็นตาราง)
./parallel -o csv -H -n 800 -t 8       # แสดงเป็น CSV พร้อมหัวตาราง
```

| ตัวเลือก | ความหมาย | ค่าเริ่มต้น |
|---|---|---|
| `-n N` | ขนาดเมตริกซ์ n×n (1–4096) | 800 |
| `-t T` | จำนวน OpenMP threads | `OMP_NUM_THREADS` หรือจำนวน CPU |
| `-a ALGO` | `naive` \| `ikj` \| `transpose` \| `blocked` | `ikj` |
| `-b BS` | ขนาด block ของ `blocked` | 64 |
| `-s SCHED` | OpenMP schedule: `static` \| `dynamic` \| `guided` | `static` |
| `-c CHUNK` | chunk size ของ schedule (0 = ค่า default ของ OpenMP) | 0 |
| `-r R` | ทำ kernel ซ้ำ R รอบ แล้วรายงานค่าที่ดีที่สุดและค่าเฉลี่ย | 3 |
| `-v` | ตรวจผลลัพธ์กับการคำนวณแบบ serial (n ≤ 800 ตรวจทั้งเมตริกซ์, n > 800 สุ่มตรวจ 2,000 ช่อง) | ปิด |
| `-l LABEL` | ข้อความกำกับในคอลัมน์แรกของ CSV | `-` |
| `-o FORMAT` | รูปแบบผลลัพธ์: `table` \| `csv` | `table` ถ้าแสดงบนเทอร์มินัล, `csv` ถ้า redirect/pipe |
| `-H` | พิมพ์บรรทัดหัวตาราง CSV ก่อน (เฉพาะ `-o csv`) | ปิด |

เมื่อรันบนเทอร์มินัล ผลลัพธ์จะแสดงเป็นตาราง 3 ส่วน ได้แก่ Configuration, Timing (เวลาแต่ละช่วง serial/parallel
พร้อมสัดส่วน %) และ Results (GFLOP/s, หน่วยความจำ, checksum, ผลการตรวจ) ตัวอย่างเช่น
```
 Timing
┌──────────────────────┬──────────┬──────────┬─────────┐
│ Phase                │ Type     │ Time (s) │   Share │
├──────────────────────┼──────────┼──────────┼─────────┤
│ init (malloc + fill) │ serial   │ 0.005990 │   1.4 % │
│ multiply (best)      │ parallel │ 0.040169 │   9.1 % │
│ multiply (mean)      │ parallel │ 0.042118 │       - │
│ checksum             │ serial   │ 0.000563 │   0.1 % │
│ verify               │ serial   │ 0.395912 │  89.4 % │
├──────────────────────┼──────────┼──────────┼─────────┤
│ total                │          │ 0.442634 │ 100.0 % │
└──────────────────────┴──────────┴──────────┴─────────┘
```

เมื่อ redirect ลงไฟล์หรือ pipe (หรือใช้ `-o csv`) ผลลัพธ์คือ CSV หนึ่งบรรทัดต่อการรันหนึ่งครั้ง คอลัมน์หลักได้แก่
`t_init` (serial), `t_mul_best` / `t_mul_mean` (ส่วน parallel), `t_post` (serial), `gflops`,
`mem_theory_mb`, `maxrss_mb`, `vmpeak_mb`, `checksum`, `verified` (1 = ถูกต้อง, 0 = ผิด, -1 = ไม่ได้ตรวจ)

ค่าที่ตั้งผ่าน environment variable ของ OpenMP ก็ใช้ได้ตามปกติ เช่น
```sh
OMP_PROC_BIND=close OMP_PLACES=cores ./parallel -n 1600 -t 8
```

## ทำการทดลองทั้งหมดซ้ำ (ไม่บังคับ)
```sh
./bench.sh RESULTS_DIR          # รันทุกการทดลอง ~45–60 นาที เขียน CSV ลง RESULTS_DIR/raw/
./bench.sh RESULTS_DIR quick    # รุ่นย่อ ~15 วินาที ไว้ทดสอบสคริปต์
python3 plot.py RESULTS_DIR     # สร้างกราฟ RESULTS_DIR/figures/*.png และ RESULTS_DIR/summary.md
```
`bench.sh` คอมไพล์โปรแกรมเองลงใน `RESULTS_DIR/bin/` ไม่ต้องใช้สิทธิ์ root และไม่เปลี่ยนการตั้งค่าใดๆ
ของระบบ (เช่น CPU governor จะถูกบันทึกไว้ใน `sysinfo.txt` เท่านั้น)

## ปัญหาที่พบบ่อยและวิธีแก้
| อาการ | สาเหตุ | วิธีแก้ |
|---|---|---|
| `fatal error: omp.h: No such file or directory` หรือ `clang: error: unsupported option '-fopenmp'` (macOS) | Apple clang ไม่มี OpenMP ในตัว และคำสั่ง `gcc` บน macOS คือ clang | `brew install libomp` แล้วใช้ `make` (อย่าพิมพ์คำสั่ง `gcc` เอง) หรือ `brew install gcc && make CC=gcc-14` |
| `error: libomp not found` ตอน `make` (macOS) | ยังไม่ได้ติดตั้ง libomp หรือไม่มี Homebrew | ติดตั้ง Homebrew จาก https://brew.sh แล้ว `brew install libomp` |
| `sys/resource.h: No such file or directory` (Windows, MinGW/MSYS2) หรือ `make` แจ้ง `build this program inside WSL2` | Windows ไม่มี `getrusage()` | คอมไพล์และรันใน WSL2 |
| `undefined reference to 'omp_get_max_threads'` (หรือฟังก์ชัน `omp_*` อื่น) | คอมไพล์โดยไม่มี `-fopenmp` | ใช้ `make` |
| `Makefile:…: *** missing separator` | บรรทัดคำสั่งใน `Makefile` ถูกแก้จาก Tab เป็นช่องว่าง | ใช้ `Makefile` ต้นฉบับ (บรรทัดคำสั่งต้องขึ้นต้นด้วย Tab) |
| `/usr/bin/env: 'bash\r': No such file or directory` ตอนรัน `bench.sh` | ไฟล์ถูกเปิด/บันทึกบน Windows แล้วกลายเป็น CRLF | `sed -i 's/\r$//' bench.sh` |
| `error: -n must be an integer in [1, 4096]` / `-t must be …` | ค่าเกินช่วงที่โปรแกรมยอมรับ (threads ได้สูงสุด 4 × จำนวน CPU) | ใช้ค่าในช่วงที่แจ้ง |
| ได้ CSV แทนตาราง | ผลลัพธ์ถูก redirect/pipe หรือรันในเทอร์มินัลที่โปรแกรมไม่รู้ว่าเป็นเทอร์มินัล | เพิ่ม `-o table` |
| `bench.sh`: `taskset: command not found` / `lscpu: command not found` (macOS) | สคริปต์ใช้เครื่องมือของ Linux | รันใน Linux หรือ WSL2 เท่านั้น |
| `Peak RSS` สูงผิดปกติ และ `VmPeak` เป็น n/a / `-1` (macOS) | หน่วยของ `ru_maxrss` ต่างจาก Linux และไม่มี `/proc` | เป็นข้อจำกัดที่ทราบแล้ว ใช้ค่าหน่วยความจำจาก Linux/WSL2 แทน |
| เวลาแกว่งมากระหว่างการรัน | มีโปรแกรมอื่นแย่ง CPU, เครื่องอยู่ในโหมดประหยัดพลังงาน หรือความร้อนทำให้ CPU ลดความเร็ว | ปิดโปรแกรมอื่น เสียบสายชาร์จ และใช้ `-r` เพื่อรันซ้ำหลายรอบ |

## สิ่งที่ห้ามทำ
- **ห้ามรันด้วย `sudo`** โปรแกรมและ `bench.sh` ไม่ต้องใช้สิทธิ์ root และไม่ควรได้สิทธิ์เกินจำเป็น
- **ห้ามรัน `bench.sh` แบบเต็มบนเครื่องที่ใช้ร่วมกับผู้อื่น** (เช่น server ของภาควิชา) โดยไม่ได้รับอนุญาต
  เพราะใช้ CPU ทุก core ต่อเนื่อง 45–60 นาที และไม่ควรรันขณะใช้แบตเตอรี่
- **ห้ามนำเวลาที่วัดบน macOS, WSL2 หรือ VM ไปเทียบกับตัวเลขในรายงานโดยตรง** เพราะ CPU, scheduler
  และ OpenMP runtime ต่างกัน ผลในรายงานวัดบน Linux เท่านั้น
- **ห้ามแก้ `Makefile` ให้บรรทัดคำสั่งใช้ช่องว่างแทน Tab** และห้ามคอมไพล์โดยตัด `-fopenmp` ออก
- **ห้ามส่งไฟล์ที่คอมไพล์แล้ว** (`parallel`, `parallel.exe`) หรือโฟลเดอร์ผลการทดลองไว้ในโฟลเดอร์ส่งงาน
  (โจทย์หักคะแนนเมื่อมีไฟล์เกิน)

## สิ่งที่ควรทำ
- ใช้ `make` แทนการพิมพ์คำสั่งคอมไพล์เอง เพื่อให้ได้ flag ที่ถูกต้องตามระบบ
- หลังคอมไพล์บนเครื่องใหม่ ให้ตรวจความถูกต้องก่อนด้วย `./parallel -n 333 -t 4 -a blocked -v` (ต้องได้ `PASS`)
- ดูจำนวน CPU ด้วย `nproc` (Linux/WSL2) หรือ `sysctl -n hw.ncpu` (macOS) ก่อนเลือกค่า `-t`
- บน WSL2 ให้คัดลอกโฟลเดอร์ไปไว้ใน home ของ Linux (`~`) แทนการรันจาก `/mnt/c` จะอ่านเขียนไฟล์ได้เร็วกว่า
- รัน `make clean` ก่อนบีบอัดหรือส่งงานทุกครั้ง
