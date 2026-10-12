# `make` builds ./parallel (single-file C + OpenMP) on Linux (also WSL2) and macOS.
CFLAGS  ?= -O2 -Wall -Wextra
OMPFLAGS = -fopenmp
OMPLIBS  =
OMPCHECK = true

ifeq ($(OS),Windows_NT)
  # Native Windows lacks <sys/resource.h> (getrusage); build inside WSL2 instead.
  OMPCHECK = { echo "error: build this program inside WSL2 on Windows (see BUILD.md)"; exit 1; }
else
  UNAME_S := $(shell uname -s)
  ifeq ($(UNAME_S),Darwin)
    # macOS: Apple clang has no OpenMP runtime of its own, so use Homebrew's libomp
    # (brew install libomp). With Homebrew gcc instead: make CC=gcc-14 (plain -fopenmp).
    ifneq ($(origin CC),command line)
      CC       = clang
      LIBOMP  := $(shell brew --prefix libomp 2>/dev/null)
      OMPFLAGS = -Xpreprocessor -fopenmp -I$(LIBOMP)/include
      OMPLIBS  = -L$(LIBOMP)/lib -lomp
      ifeq ($(LIBOMP),)
        OMPCHECK = { echo "error: libomp not found. Install it with: brew install libomp"; echo "       (or use Homebrew gcc: brew install gcc && make CC=gcc-14)"; exit 1; }
      endif
    endif
  else
    CC = gcc
  endif
endif

.PHONY: all clean

all: parallel

parallel: parallel.c
	@$(OMPCHECK)
	$(CC) $(CFLAGS) $(OMPFLAGS) -o $@ parallel.c $(OMPLIBS) -lm

clean:
	rm -f parallel
