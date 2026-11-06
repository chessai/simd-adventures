### lookup: ns per operation (lower is better), AVX2 backend, uniform keys, mixed 50/50 queries

| structure | API | 256 KiB | 1 MiB | 16 MiB | 1024 MiB |
|---|---|---|---|---|---|
| classic-k7 | per-key | 21.66 | 22.88 | 32.50 | 85.83 |
| blocked32 | per-key | 4.95 | 5.19 | 9.90 | 31.56 |
| blocked32 | batch | 3.01 | 3.01 | 7.54 | 11.26 |
| blocked64 | per-key | 6.72 | 6.81 | 10.68 | 30.48 |
| blocked64 | batch | 3.80 | 3.83 | 8.05 | 11.71 |
| blocked64x2 | per-key | 8.44 | 8.57 | 11.10 | 31.83 |
| blocked64x2 | batch | 4.95 | 4.81 | 10.17 | 13.53 |
| cuckoo16 | per-key | 7.30 | 7.38 | 12.94 | 48.04 |
| cuckoo16 | batch | 4.41 | 4.47 | 11.40 | 16.34 |
| cuckoo8 | per-key | 6.77 | 6.97 | 12.33 | 43.03 |
| cuckoo8 | batch | 4.40 | 4.58 | 11.62 | 15.93 |

### insert: ns per operation (lower is better), AVX2 backend, uniform keys, mixed 50/50 queries

| structure | API | 256 KiB | 1 MiB | 16 MiB | 1024 MiB |
|---|---|---|---|---|---|
| classic-k7 | per-key | 17.67 | 13.83 | 23.11 | 106.89 |
| blocked32 | per-key | 3.96 | 3.94 | 7.76 | 27.50 |
| blocked32 | batch | 2.48 | 2.34 | 3.80 | 7.25 |
| blocked64 | per-key | 5.15 | 4.93 | 7.72 | 27.80 |
| blocked64 | batch | 2.79 | 3.14 | 4.26 | 7.96 |
| blocked64x2 | per-key | 6.50 | 6.53 | 8.12 | 28.98 |
| blocked64x2 | batch | 3.94 | 3.97 | 5.69 | 9.00 |
| cuckoo16 | per-key | 22.28 | 19.09 | 60.32 | 167.52 |
| cuckoo16 | batch | 24.50 | 22.40 | 53.38 | 132.58 |
| cuckoo8 | per-key | 10.41 | 10.84 | 18.09 | 53.77 |
| cuckoo8 | batch | 9.03 | 9.26 | 12.26 | 26.13 |

### Backend comparison: batched lookup ns/op (uniform keys, mixed queries)

| structure | API | size | portable | avx2 | avx512 |
|---|---|---|---|---|---|
| classic-k7 | per-key | 256 KiB | – | 21.66 | – |
| classic-k7 | per-key | 1 MiB | – | 22.88 | – |
| classic-k7 | per-key | 16 MiB | – | 32.50 | – |
| classic-k7 | per-key | 1024 MiB | – | 85.83 | – |
| blocked32 | per-key | 256 KiB | 14.18 | 4.95 | 4.78 |
| blocked32 | per-key | 1 MiB | 14.99 | 5.19 | 5.38 |
| blocked32 | per-key | 16 MiB | 24.60 | 9.90 | 10.41 |
| blocked32 | per-key | 1024 MiB | 59.78 | 31.56 | 29.46 |
| blocked32 | batch | 256 KiB | 10.92 | 3.01 | 3.90 |
| blocked32 | batch | 1 MiB | 11.71 | 3.01 | 4.16 |
| blocked32 | batch | 16 MiB | 15.07 | 7.54 | 9.06 |
| blocked32 | batch | 1024 MiB | 22.61 | 11.26 | 11.59 |
| blocked64 | per-key | 256 KiB | 14.44 | 6.72 | 5.27 |
| blocked64 | per-key | 1 MiB | 16.95 | 6.81 | 5.67 |
| blocked64 | per-key | 16 MiB | 21.75 | 10.68 | 9.97 |
| blocked64 | per-key | 1024 MiB | 67.40 | 30.48 | 30.24 |
| blocked64 | batch | 256 KiB | 11.45 | 3.80 | 4.32 |
| blocked64 | batch | 1 MiB | 12.07 | 3.83 | 4.18 |
| blocked64 | batch | 16 MiB | 14.75 | 8.05 | 7.94 |
| blocked64 | batch | 1024 MiB | 21.35 | 11.71 | 11.39 |
| blocked64x2 | per-key | 256 KiB | 17.68 | 8.44 | 6.10 |
| blocked64x2 | per-key | 1 MiB | 19.35 | 8.57 | 6.13 |
| blocked64x2 | per-key | 16 MiB | 28.42 | 11.10 | 9.76 |
| blocked64x2 | per-key | 1024 MiB | 70.71 | 31.83 | 28.60 |
| blocked64x2 | batch | 256 KiB | 14.08 | 4.95 | 5.54 |
| blocked64x2 | batch | 1 MiB | 14.56 | 4.81 | 5.28 |
| blocked64x2 | batch | 16 MiB | 18.51 | 10.17 | 8.83 |
| blocked64x2 | batch | 1024 MiB | 23.18 | 13.53 | 12.28 |
| cuckoo16 | per-key | 256 KiB | 13.19 | 7.30 | 7.65 |
| cuckoo16 | per-key | 1 MiB | 14.44 | 7.38 | 7.70 |
| cuckoo16 | per-key | 16 MiB | 22.59 | 12.94 | 24.45 |
| cuckoo16 | per-key | 1024 MiB | 73.33 | 48.04 | 51.03 |
| cuckoo16 | batch | 256 KiB | 12.02 | 4.41 | 4.28 |
| cuckoo16 | batch | 1 MiB | 12.74 | 4.47 | 4.56 |
| cuckoo16 | batch | 16 MiB | 19.37 | 11.40 | 11.16 |
| cuckoo16 | batch | 1024 MiB | 25.55 | 16.34 | 16.22 |
| cuckoo8 | per-key | 256 KiB | – | 6.77 | – |
| cuckoo8 | per-key | 1 MiB | – | 6.97 | – |
| cuckoo8 | per-key | 16 MiB | – | 12.33 | – |
| cuckoo8 | per-key | 1024 MiB | – | 43.03 | – |
| cuckoo8 | batch | 256 KiB | – | 4.40 | – |
| cuckoo8 | batch | 1 MiB | – | 4.58 | – |
| cuckoo8 | batch | 16 MiB | – | 11.62 | – |
| cuckoo8 | batch | 1024 MiB | – | 15.93 | – |

### Query distribution: batched lookup ns/op, AVX2 (uniform keys)

| structure | API | size | absent (hit rate) | mixed (hit rate) | zipf (hit rate) |
|---|---|---|---|---|---|
| classic-k7 | per-key | 1 MiB | 28.36 (0.01) | 22.88 (0.50) | 12.88 (1.00) |
| classic-k7 | per-key | 16 MiB | 36.93 (0.01) | 32.50 (0.50) | 19.79 (1.00) |
| blocked32 | per-key | 1 MiB | 5.34 (0.01) | 5.19 (0.51) | 5.32 (1.00) |
| blocked32 | per-key | 16 MiB | 9.38 (0.01) | 9.90 (0.51) | 8.08 (1.00) |
| blocked32 | batch | 1 MiB | 3.07 (0.01) | 3.01 (0.51) | 3.19 (1.00) |
| blocked32 | batch | 16 MiB | 7.19 (0.01) | 7.54 (0.51) | 5.76 (1.00) |
| blocked64 | per-key | 1 MiB | 7.03 (0.01) | 6.81 (0.51) | 6.88 (1.00) |
| blocked64 | per-key | 16 MiB | 10.13 (0.01) | 10.68 (0.51) | 9.52 (1.00) |
| blocked64 | batch | 1 MiB | 4.02 (0.01) | 3.83 (0.51) | 3.98 (1.00) |
| blocked64 | batch | 16 MiB | 8.10 (0.01) | 8.05 (0.51) | 6.78 (1.00) |
| blocked64x2 | per-key | 1 MiB | 8.22 (0.04) | 8.57 (0.52) | 8.45 (1.00) |
| blocked64x2 | per-key | 16 MiB | 10.76 (0.04) | 11.10 (0.52) | 10.48 (1.00) |
| blocked64x2 | batch | 1 MiB | 4.79 (0.04) | 4.81 (0.52) | 5.05 (1.00) |
| blocked64x2 | batch | 16 MiB | 9.20 (0.04) | 10.17 (0.52) | 7.67 (1.00) |
| cuckoo16 | per-key | 1 MiB | 7.25 (0.00) | 7.38 (0.50) | 7.57 (1.00) |
| cuckoo16 | per-key | 16 MiB | 12.98 (0.00) | 12.94 (0.50) | 12.98 (1.00) |
| cuckoo16 | batch | 1 MiB | 4.53 (0.00) | 4.47 (0.50) | 4.36 (1.00) |
| cuckoo16 | batch | 16 MiB | 11.57 (0.00) | 11.40 (0.50) | 9.50 (1.00) |
| cuckoo8 | per-key | 1 MiB | 7.06 (0.02) | 6.97 (0.51) | 6.96 (1.00) |
| cuckoo8 | per-key | 16 MiB | 11.87 (0.02) | 12.33 (0.51) | 12.05 (1.00) |
| cuckoo8 | batch | 1 MiB | 4.47 (0.02) | 4.58 (0.51) | 4.23 (1.00) |
| cuckoo8 | batch | 16 MiB | 11.05 (0.02) | 11.62 (0.51) | 9.18 (1.00) |

### Key distribution: lookup ns/op and hit rate, AVX2 (mixed queries)

| structure | API | size | uniform (hit rate) | sequential (hit rate) | clustered (hit rate) |
|---|---|---|---|---|---|
| classic-k7 | per-key | 1 MiB | 22.88 (0.504) | 23.56 (0.504) | 23.02 (0.504) |
| classic-k7 | per-key | 16 MiB | 32.50 (0.504) | 32.90 (0.504) | 31.93 (0.504) |
| blocked32 | per-key | 1 MiB | 5.19 (0.506) | 5.51 (0.506) | 5.24 (0.506) |
| blocked32 | per-key | 16 MiB | 9.90 (0.506) | 9.94 (0.506) | 9.30 (0.506) |
| blocked32 | batch | 1 MiB | 3.01 (0.506) | 3.00 (0.506) | 2.98 (0.506) |
| blocked32 | batch | 16 MiB | 7.54 (0.506) | 7.76 (0.506) | 7.37 (0.506) |
| blocked64 | per-key | 1 MiB | 6.81 (0.505) | 6.99 (0.505) | 6.77 (0.505) |
| blocked64 | per-key | 16 MiB | 10.68 (0.505) | 11.10 (0.505) | 11.11 (0.505) |
| blocked64 | batch | 1 MiB | 3.83 (0.505) | 3.76 (0.505) | 3.73 (0.505) |
| blocked64 | batch | 16 MiB | 8.05 (0.505) | 8.19 (0.505) | 8.73 (0.505) |
| blocked64x2 | per-key | 1 MiB | 8.57 (0.518) | 8.27 (0.518) | 8.24 (0.518) |
| blocked64x2 | per-key | 16 MiB | 11.10 (0.518) | 11.15 (0.518) | 11.05 (0.518) |
| blocked64x2 | batch | 1 MiB | 4.81 (0.518) | 4.95 (0.518) | 4.93 (0.518) |
| blocked64x2 | batch | 16 MiB | 10.17 (0.518) | 9.20 (0.518) | 9.54 (0.518) |
| cuckoo16 | per-key | 1 MiB | 7.38 (0.500) | 7.61 (0.500) | 8.08 (0.500) |
| cuckoo16 | per-key | 16 MiB | 12.94 (0.500) | 12.42 (0.500) | 13.18 (0.500) |
| cuckoo16 | batch | 1 MiB | 4.47 (0.500) | 4.57 (0.500) | 4.59 (0.500) |
| cuckoo16 | batch | 16 MiB | 11.40 (0.500) | 11.19 (0.500) | 11.37 (0.500) |
| cuckoo8 | per-key | 1 MiB | 6.97 (0.512) | 7.23 (0.512) | 7.05 (0.512) |
| cuckoo8 | per-key | 16 MiB | 12.33 (0.512) | 12.57 (0.512) | 13.25 (0.512) |
| cuckoo8 | batch | 1 MiB | 4.58 (0.512) | 4.61 (0.512) | 4.34 (0.512) |
| cuckoo8 | batch | 16 MiB | 11.62 (0.512) | 11.39 (0.512) | 11.49 (0.512) |

### Prefetch window sweep: batched lookup ns/op vs window size W (keys hashed+prefetched one window ahead)

| structure | size | W=1 | W=4 | W=8 | W=16 | W=32 | W=64 | W=128 | W=256 |
|---|---|---|---|---|---|---|---|---|---|
| blocked32 | 1 MiB | 8.96 | 5.21 | 3.37 | 3.09 | 3.02 | 3.42 | 3.18 | 4.10 |
| blocked32 | 16 MiB | 13.71 | 8.46 | 8.95 | 7.70 | 7.27 | 6.28 | 6.44 | 6.07 |
| blocked32 | 1024 MiB | 39.49 | 18.46 | 12.98 | 10.62 | 10.73 | 10.60 | 8.27 | 8.37 |
| cuckoo16 | 1 MiB | 11.30 | 6.97 | 5.03 | 5.08 | 4.46 | 4.36 | 4.84 | 5.34 |
| cuckoo16 | 16 MiB | 17.24 | 10.30 | 12.35 | 13.28 | 11.14 | 11.25 | 9.70 | 9.15 |
| cuckoo16 | 1024 MiB | 58.63 | 24.26 | 19.54 | 19.09 | 16.13 | 15.99 | 13.73 | 14.31 |

### Transparent huge pages on/off at 1 GiB (AVX2, uniform, mixed): ns/op

| structure | API | op | THP (madvise) | 4 KiB pages | ratio |
|---|---|---|---|---|---|
| classic-k7 | per-key | lookup | 85.83 | 115.56 | 1.35x |
| classic-k7 | per-key | insert | 106.89 | 129.65 | 1.21x |
| blocked32 | per-key | lookup | 31.56 | 32.13 | 1.02x |
| blocked32 | per-key | insert | 27.50 | 31.92 | 1.16x |
| blocked32 | batch | lookup | 11.26 | 17.47 | 1.55x |
| blocked32 | batch | insert | 7.25 | 15.46 | 2.13x |
| cuckoo16 | per-key | lookup | 48.04 | 53.41 | 1.11x |
| cuckoo16 | per-key | insert | 167.52 | 209.48 | 1.25x |
| cuckoo16 | batch | lookup | 16.34 | 33.77 | 2.07x |
| cuckoo16 | batch | insert | 132.58 | 183.32 | 1.38x |

_Hardware counters were not available in the environment where these numbers were collected (perf_event_open returned ENOENT inside the KVM guest); cycles/op in the CSV are TSC ticks at the nominal 2.1 GHz._

### fpr.txt

```
### build/fpr_avx512 uniform
dist=uniform  memory=2097152 bytes for every structure, 4194304 absent queries
   bpk     n keys |    classic  blocked32  blocked64    blk64x2 |    cuckoo8(load)   cuckoo16(load)
                  |      k=opt        k=8        k=8       k=16 |              FPR              FPR
   8.5    1973790 |    0.01692    0.02571    0.02229    0.08453 |   0.02921 (0.94)       n/a (1.88)   (classic k=6)
  10.0    1677721 |    0.00818    0.01256    0.01043    0.03647 |   0.02490 (0.80)       n/a (1.60)   (classic k=7)
  12.0    1398101 |    0.00310    0.00535    0.00418    0.01228 |   0.02080 (0.67)       n/a (1.33)   (classic k=8)
  14.0    1198372 |    0.00118    0.00253    0.00187    0.00439 |   0.01788 (0.57)       n/a (1.14)   (classic k=10)
  17.0     986895 |    0.00028    0.00095    0.00065    0.00108 |   0.01473 (0.47)  0.000107 (0.94)   (classic k=12)
  20.0     838860 |    0.00006    0.00041    0.00026    0.00030 |   0.01252 (0.40)  0.000090 (0.80)   (classic k=14)
  24.0     699050 |    0.00001    0.00016    0.00009    0.00006 |   0.01045 (0.33)  0.000079 (0.67)   (classic k=17)
  32.0     524288 |    0.00000    0.00003    0.00002    0.00000 |   0.00786 (0.25)  0.000059 (0.50)   (classic k=22)
### build/fpr_avx512 sequential
dist=sequential  memory=2097152 bytes for every structure, 4194304 absent queries
   bpk     n keys |    classic  blocked32  blocked64    blk64x2 |    cuckoo8(load)   cuckoo16(load)
                  |      k=opt        k=8        k=8       k=16 |              FPR              FPR
   8.5    1973790 |    0.01681    0.02588    0.02249    0.08555 |   0.02915 (0.94)       n/a (1.88)   (classic k=6)
  10.0    1677721 |    0.00818    0.01272    0.01056    0.03708 |   0.02488 (0.80)       n/a (1.60)   (classic k=7)
  12.0    1398101 |    0.00313    0.00544    0.00421    0.01245 |   0.02080 (0.67)       n/a (1.33)   (classic k=8)
  14.0    1198372 |    0.00120    0.00257    0.00190    0.00445 |   0.01788 (0.57)       n/a (1.14)   (classic k=10)
  17.0     986895 |    0.00028    0.00097    0.00066    0.00108 |   0.01472 (0.47)  0.000114 (0.94)   (classic k=12)
  20.0     838860 |    0.00007    0.00042    0.00027    0.00032 |   0.01249 (0.40)  0.000096 (0.80)   (classic k=14)
  24.0     699050 |    0.00001    0.00016    0.00010    0.00007 |   0.01043 (0.33)  0.000081 (0.67)   (classic k=17)
  32.0     524288 |    0.00000    0.00004    0.00002    0.00001 |   0.00781 (0.25)  0.000062 (0.50)   (classic k=22)
### build/fpr_avx512 clustered
dist=clustered  memory=2097152 bytes for every structure, 4194304 absent queries
   bpk     n keys |    classic  blocked32  blocked64    blk64x2 |    cuckoo8(load)   cuckoo16(load)
                  |      k=opt        k=8        k=8       k=16 |              FPR              FPR
   8.5    1973790 |    0.01686    0.02565    0.02242    0.08547 |   0.02923 (0.94)       n/a (1.88)   (classic k=6)
  10.0    1677721 |    0.00815    0.01259    0.01046    0.03683 |   0.02481 (0.80)       n/a (1.60)   (classic k=7)
  12.0    1398101 |    0.00315    0.00542    0.00420    0.01238 |   0.02075 (0.67)       n/a (1.33)   (classic k=8)
  14.0    1198372 |    0.00121    0.00254    0.00187    0.00446 |   0.01781 (0.57)       n/a (1.14)   (classic k=10)
  17.0     986895 |    0.00029    0.00095    0.00064    0.00107 |   0.01476 (0.47)  0.000108 (0.94)   (classic k=12)
  20.0     838860 |    0.00006    0.00041    0.00026    0.00030 |   0.01256 (0.40)  0.000094 (0.80)   (classic k=14)
  24.0     699050 |    0.00001    0.00016    0.00009    0.00007 |   0.01047 (0.33)  0.000083 (0.67)   (classic k=17)
  32.0     524288 |    0.00000    0.00004    0.00002    0.00001 |   0.00785 (0.25)  0.000065 (0.50)   (classic k=22)
```

### membench.txt

```
### build/membench 1024
hugepages=madvise
working-set   latency(1)   mlp=4    mlp=8   mlp=16   mlp=32   [ns per random 64B line]
      1 MiB        8.6     2.04     1.03     0.59     0.52
      4 MiB       58.0    10.79     6.57     2.69     1.58
     16 MiB       74.1    17.06     6.27     2.47     1.97
     64 MiB      112.4    30.12    13.26     6.16     3.64
    256 MiB      119.5    30.83    15.82     8.15     5.33
   1024 MiB      118.5    31.34    15.99     8.35     6.13
```

### membench_nohuge.txt

```
### build/membench 1024 --no-hugepages
hugepages=off
working-set   latency(1)   mlp=4    mlp=8   mlp=16   mlp=32   [ns per random 64B line]
      1 MiB        7.5     1.88     1.03     0.59     0.52
      4 MiB       78.6    10.61     5.41     2.43     1.44
     16 MiB      105.5    39.09    10.75     3.30     3.90
     64 MiB      125.9    33.57    15.07     7.67     4.87
    256 MiB      143.6    36.52    19.54    10.85    10.31
   1024 MiB      156.7    40.27    20.77    12.17    12.68
```
