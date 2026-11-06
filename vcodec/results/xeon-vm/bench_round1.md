group	impl	size	dist	bytes	ns	GB/s
base64_decode	base64-crate	1KiB	random	1368	539.9	2.534
base64_decode	base64-crate	4MiB	random	5592408	2.144e+06	2.608
base64_decode	base64-crate	64B	random	88	40.14	2.192
base64_decode	base64-crate	64KiB	random	87384	3.307e+04	2.642
base64_decode	base64-simd	1KiB	random	1368	149.7	9.138
base64_decode	base64-simd	4MiB	random	5592408	6.347e+05	8.811
base64_decode	base64-simd	64B	random	88	15.63	5.629
base64_decode	base64-simd	64KiB	random	87384	8950	9.763
base64_decode	openssl-EVP_DecodeBlock	1KiB	random	1368	571.6	2.393
base64_decode	openssl-EVP_DecodeBlock	4MiB	random	5592408	2.456e+06	2.277
base64_decode	openssl-EVP_DecodeBlock	64B	random	88	42.45	2.073
base64_decode	openssl-EVP_DecodeBlock	64KiB	random	87384	3.626e+04	2.41
base64_decode	vcodec-avx2	1KiB	random	1368	118.4	11.56
base64_decode	vcodec-avx2	4MiB	random	5592408	3.75e+05	14.91
base64_decode	vcodec-avx2	64B	random	88	52.09	1.689
base64_decode	vcodec-avx2	64KiB	random	87384	5313	16.45
base64_decode	vcodec-portable	1KiB	random	1368	2000	0.6839
base64_decode	vcodec-portable	4MiB	random	5592408	8.01e+06	0.6982
base64_decode	vcodec-portable	64B	random	88	138.4	0.6359
base64_decode	vcodec-portable	64KiB	random	87384	1.277e+05	0.6841
base64_decode	vcodec-scalar	1KiB	random	1368	753.3	1.816
base64_decode	vcodec-scalar	4MiB	random	5592408	2.928e+06	1.91
base64_decode	vcodec-scalar	64B	random	88	61.8	1.424
base64_decode	vcodec-scalar	64KiB	random	87384	4.426e+04	1.974
base64_decode	vcodec-sse4.1	1KiB	random	1368	160.8	8.506
base64_decode	vcodec-sse4.1	4MiB	random	5592408	5.796e+05	9.648
base64_decode	vcodec-sse4.1	64B	random	88	33.2	2.651
base64_decode	vcodec-sse4.1	64KiB	random	87384	1.036e+04	8.438
base64_encode	base64-crate	1KiB	random	1024	383.7	2.669
base64_encode	base64-crate	4MiB	random	4194304	1.57e+06	2.671
base64_encode	base64-crate	64B	random	64	31.6	2.025
base64_encode	base64-crate	64KiB	random	65536	2.518e+04	2.603
base64_encode	base64-simd	1KiB	random	1024	107	9.572
base64_encode	base64-simd	4MiB	random	4194304	4.08e+05	10.28
base64_encode	base64-simd	64B	random	64	12.89	4.964
base64_encode	base64-simd	64KiB	random	65536	6531	10.03
base64_encode	openssl-EVP_EncodeBlock	1KiB	random	1024	532.8	1.922
base64_encode	openssl-EVP_EncodeBlock	4MiB	random	4194304	2.159e+06	1.943
base64_encode	openssl-EVP_EncodeBlock	64B	random	64	39.55	1.618
base64_encode	openssl-EVP_EncodeBlock	64KiB	random	65536	3.4e+04	1.928
base64_encode	vcodec-avx2	1KiB	random	1024	83.41	12.28
base64_encode	vcodec-avx2	4MiB	random	4194304	3.358e+05	12.49
base64_encode	vcodec-avx2	64B	random	64	28.53	2.244
base64_encode	vcodec-avx2	64KiB	random	65536	3765	17.41
base64_encode	vcodec-portable	1KiB	random	1024	2039	0.5021
base64_encode	vcodec-portable	4MiB	random	4194304	8.508e+06	0.493
base64_encode	vcodec-portable	64B	random	64	141.1	0.4536
base64_encode	vcodec-portable	64KiB	random	65536	1.437e+05	0.4562
base64_encode	vcodec-scalar	1KiB	random	1024	761.9	1.344
base64_encode	vcodec-scalar	4MiB	random	4194304	2.964e+06	1.415
base64_encode	vcodec-scalar	64B	random	64	57.01	1.123
base64_encode	vcodec-scalar	64KiB	random	65536	4.484e+04	1.461
base64_encode	vcodec-sse4.1	1KiB	random	1024	133.3	7.682
base64_encode	vcodec-sse4.1	4MiB	random	4194304	5.506e+05	7.617
base64_encode	vcodec-sse4.1	64B	random	64	23.11	2.769
base64_encode	vcodec-sse4.1	64KiB	random	65536	7230	9.065
hex_decode	hex-crate	1KiB	random	2048	1954	1.048
hex_decode	hex-crate	4MiB	random	8388608	3.648e+07	0.23
hex_decode	hex-crate	64B	random	128	144.4	0.8861
hex_decode	hex-crate	64KiB	random	131072	5.222e+05	0.251
hex_decode	hex-simd	1KiB	random	2048	93.67	21.86
hex_decode	hex-simd	4MiB	random	8388608	4.924e+05	17.03
hex_decode	hex-simd	64B	random	128	5.757	22.23
hex_decode	hex-simd	64KiB	random	131072	6403	20.47
hex_decode	vcodec-avx2	1KiB	random	2048	164.5	12.45
hex_decode	vcodec-avx2	4MiB	random	8388608	6.318e+05	13.28
hex_decode	vcodec-avx2	64B	random	128	12.81	9.991
hex_decode	vcodec-avx2	64KiB	random	131072	9797	13.38
hex_decode	vcodec-portable	1KiB	random	2048	823.7	2.486
hex_decode	vcodec-portable	4MiB	random	8388608	3.385e+06	2.478
hex_decode	vcodec-portable	64B	random	128	50.05	2.557
hex_decode	vcodec-portable	64KiB	random	131072	5.792e+04	2.263
hex_decode	vcodec-scalar	1KiB	random	2048	1090	1.879
hex_decode	vcodec-scalar	4MiB	random	8388608	4.253e+06	1.972
hex_decode	vcodec-scalar	64B	random	128	67.5	1.896
hex_decode	vcodec-scalar	64KiB	random	131072	6.175e+04	2.123
hex_decode	vcodec-sse4.1	1KiB	random	2048	308.4	6.641
hex_decode	vcodec-sse4.1	4MiB	random	8388608	1.145e+06	7.328
hex_decode	vcodec-sse4.1	64B	random	128	19.73	6.487
hex_decode	vcodec-sse4.1	64KiB	random	131072	1.925e+04	6.809
hex_encode	hex-crate	1KiB	random	1024	884.5	1.158
hex_encode	hex-crate	4MiB	random	4194304	3.162e+06	1.327
hex_encode	hex-crate	64B	random	64	61.01	1.049
hex_encode	hex-crate	64KiB	random	65536	4.426e+04	1.481
hex_encode	hex-simd	1KiB	random	1024	37.2	27.53
hex_encode	hex-simd	4MiB	random	4194304	4.722e+05	8.882
hex_encode	hex-simd	64B	random	64	3.722	17.2
hex_encode	hex-simd	64KiB	random	65536	3689	17.76
hex_encode	vcodec-avx2	1KiB	random	1024	43.29	23.66
hex_encode	vcodec-avx2	4MiB	random	4194304	4.816e+05	8.709
hex_encode	vcodec-avx2	64B	random	64	14.69	4.355
hex_encode	vcodec-avx2	64KiB	random	65536	3396	19.3
hex_encode	vcodec-portable	1KiB	random	1024	706.4	1.45
hex_encode	vcodec-portable	4MiB	random	4194304	3.269e+06	1.283
hex_encode	vcodec-portable	64B	random	64	51.15	1.251
hex_encode	vcodec-portable	64KiB	random	65536	5.332e+04	1.229
hex_encode	vcodec-scalar	1KiB	random	1024	746.7	1.371
hex_encode	vcodec-scalar	4MiB	random	4194304	3.042e+06	1.379
hex_encode	vcodec-scalar	64B	random	64	56.12	1.14
hex_encode	vcodec-scalar	64KiB	random	65536	4.701e+04	1.394
hex_encode	vcodec-sse4.1	1KiB	random	1024	83.8	12.22
hex_encode	vcodec-sse4.1	4MiB	random	4194304	4.322e+05	9.704
hex_encode	vcodec-sse4.1	64B	random	64	15.56	4.113
hex_encode	vcodec-sse4.1	64KiB	random	65536	4754	13.79
url_decode	percent-encoding	1KiB	binary	2610	1944	1.342
url_decode	percent-encoding	1KiB	pct10	1216	847.7	1.434
url_decode	percent-encoding	1KiB	safe	1024	761.4	1.345
url_decode	percent-encoding	1KiB	text	1360	1020	1.334
url_decode	percent-encoding	4MiB	binary	10422876	1.714e+07	0.6082
url_decode	percent-encoding	4MiB	pct10	5032344	9.146e+06	0.5502
url_decode	percent-encoding	4MiB	safe	4194304	2.864e+06	1.464
url_decode	percent-encoding	4MiB	text	5682444	1.235e+07	0.46
url_decode	percent-encoding	64B	binary	178	157.3	1.132
url_decode	percent-encoding	64B	pct10	76	119.4	0.6365
url_decode	percent-encoding	64B	safe	64	83.34	0.7679
url_decode	percent-encoding	64B	text	88	98.36	0.8947
url_decode	percent-encoding	64KiB	binary	162804	2.595e+05	0.6275
url_decode	percent-encoding	64KiB	pct10	78566	1.283e+05	0.6125
url_decode	percent-encoding	64KiB	safe	65536	4.977e+04	1.317
url_decode	percent-encoding	64KiB	text	88676	1.717e+05	0.5164
url_decode	vcodec-avx2	1KiB	binary	2610	838.1	3.114
url_decode	vcodec-avx2	1KiB	pct10	1216	364.7	3.334
url_decode	vcodec-avx2	1KiB	safe	1024	55.44	18.47
url_decode	vcodec-avx2	1KiB	text	1360	409.8	3.319
url_decode	vcodec-avx2	4MiB	binary	10422876	3.165e+06	3.294
url_decode	vcodec-avx2	4MiB	pct10	5032344	1.595e+06	3.156
url_decode	vcodec-avx2	4MiB	safe	4194304	2.955e+05	14.19
url_decode	vcodec-avx2	4MiB	text	5682444	1.877e+06	3.027
url_decode	vcodec-avx2	64B	binary	178	59.26	3.004
url_decode	vcodec-avx2	64B	pct10	76	29.74	2.555
url_decode	vcodec-avx2	64B	safe	64	37.35	1.714
url_decode	vcodec-avx2	64B	text	88	38.09	2.31
url_decode	vcodec-avx2	64KiB	binary	162804	5.19e+04	3.137
url_decode	vcodec-avx2	64KiB	pct10	78566	2.589e+04	3.035
url_decode	vcodec-avx2	64KiB	safe	65536	1860	35.24
url_decode	vcodec-avx2	64KiB	text	88676	2.935e+04	3.021
url_decode	vcodec-portable	1KiB	binary	2610	2770	0.9422
url_decode	vcodec-portable	1KiB	pct10	1216	1225	0.9927
url_decode	vcodec-portable	1KiB	safe	1024	70.78	14.47
url_decode	vcodec-portable	1KiB	text	1360	1659	0.8199
url_decode	vcodec-portable	4MiB	binary	10422876	1.783e+07	0.5847
url_decode	vcodec-portable	4MiB	pct10	5032344	8.478e+06	0.5936
url_decode	vcodec-portable	4MiB	safe	4194304	3.044e+05	13.78
url_decode	vcodec-portable	4MiB	text	5682444	9.885e+06	0.5748
url_decode	vcodec-portable	64B	binary	178	185.8	0.9578
url_decode	vcodec-portable	64B	pct10	76	70.81	1.073
url_decode	vcodec-portable	64B	safe	64	21.47	2.981
url_decode	vcodec-portable	64B	text	88	103.3	0.852
url_decode	vcodec-portable	64KiB	binary	162804	2.367e+05	0.6879
url_decode	vcodec-portable	64KiB	pct10	78566	9.485e+04	0.8283
url_decode	vcodec-portable	64KiB	safe	65536	3389	19.34
url_decode	vcodec-portable	64KiB	text	88676	1.296e+05	0.6842
url_decode	vcodec-scalar	1KiB	binary	2610	1677	1.557
url_decode	vcodec-scalar	1KiB	pct10	1216	779.5	1.56
url_decode	vcodec-scalar	1KiB	safe	1024	666.8	1.536
url_decode	vcodec-scalar	1KiB	text	1360	931.4	1.46
url_decode	vcodec-scalar	4MiB	binary	10422876	1.776e+07	0.5868
url_decode	vcodec-scalar	4MiB	pct10	5032344	8.916e+06	0.5644
url_decode	vcodec-scalar	4MiB	safe	4194304	2.655e+06	1.58
url_decode	vcodec-scalar	4MiB	text	5682444	1.218e+07	0.4665
url_decode	vcodec-scalar	64B	binary	178	98.64	1.805
url_decode	vcodec-scalar	64B	pct10	76	64.22	1.183
url_decode	vcodec-scalar	64B	safe	64	46.83	1.367
url_decode	vcodec-scalar	64B	text	88	63.3	1.39
url_decode	vcodec-scalar	64KiB	binary	162804	2.666e+05	0.6106
url_decode	vcodec-scalar	64KiB	pct10	78566	1.25e+05	0.6287
url_decode	vcodec-scalar	64KiB	safe	65536	4.206e+04	1.558
url_decode	vcodec-scalar	64KiB	text	88676	1.763e+05	0.5029
url_decode	vcodec-sse4.1	1KiB	binary	2610	1139	2.291
url_decode	vcodec-sse4.1	1KiB	pct10	1216	436.5	2.786
url_decode	vcodec-sse4.1	1KiB	safe	1024	90.73	11.29
url_decode	vcodec-sse4.1	1KiB	text	1360	601	2.263
url_decode	vcodec-sse4.1	4MiB	binary	10422876	4.456e+06	2.339
url_decode	vcodec-sse4.1	4MiB	pct10	5032344	2.45e+06	2.054
url_decode	vcodec-sse4.1	4MiB	safe	4194304	3.428e+05	12.24
url_decode	vcodec-sse4.1	4MiB	text	5682444	2.535e+06	2.242
url_decode	vcodec-sse4.1	64B	binary	178	75.61	2.354
url_decode	vcodec-sse4.1	64B	pct10	76	32.36	2.349
url_decode	vcodec-sse4.1	64B	safe	64	23.04	2.778
url_decode	vcodec-sse4.1	64B	text	88	42.49	2.071
url_decode	vcodec-sse4.1	64KiB	binary	162804	6.852e+04	2.376
url_decode	vcodec-sse4.1	64KiB	pct10	78566	3.017e+04	2.605
url_decode	vcodec-sse4.1	64KiB	safe	65536	4467	14.67
url_decode	vcodec-sse4.1	64KiB	text	88676	3.896e+04	2.276
url_encode	percent-encoding	1KiB	binary	1024	3929	0.2606
url_encode	percent-encoding	1KiB	pct10	1024	1512	0.6774
url_encode	percent-encoding	1KiB	safe	1024	665.2	1.539
url_encode	percent-encoding	1KiB	text	1024	2167	0.4724
url_encode	percent-encoding	4MiB	binary	4194304	4.179e+07	0.1004
url_encode	percent-encoding	4MiB	pct10	4194304	1.015e+07	0.4131
url_encode	percent-encoding	4MiB	safe	4194304	3.091e+06	1.357
url_encode	percent-encoding	4MiB	text	4194304	1.542e+07	0.2719
url_encode	percent-encoding	64B	binary	64	394.7	0.1622
url_encode	percent-encoding	64B	pct10	64	198.4	0.3226
url_encode	percent-encoding	64B	safe	64	54.4	1.176
url_encode	percent-encoding	64B	text	64	251.3	0.2546
url_encode	percent-encoding	64KiB	binary	65536	6.035e+05	0.1086
url_encode	percent-encoding	64KiB	pct10	65536	1.349e+05	0.4858
url_encode	percent-encoding	64KiB	safe	65536	4.328e+04	1.514
url_encode	percent-encoding	64KiB	text	65536	2.138e+05	0.3066
url_encode	vcodec-avx2	1KiB	binary	1024	445.9	2.296
url_encode	vcodec-avx2	1KiB	pct10	1024	402.4	2.544
url_encode	vcodec-avx2	1KiB	safe	1024	46.91	21.83
url_encode	vcodec-avx2	1KiB	text	1024	438.7	2.334
url_encode	vcodec-avx2	4MiB	binary	4194304	1.979e+06	2.12
url_encode	vcodec-avx2	4MiB	pct10	4194304	1.798e+06	2.333
url_encode	vcodec-avx2	4MiB	safe	4194304	2.789e+05	15.04
url_encode	vcodec-avx2	4MiB	text	4194304	1.803e+06	2.326
url_encode	vcodec-avx2	64B	binary	64	37.01	1.729
url_encode	vcodec-avx2	64B	pct10	64	40.67	1.574
url_encode	vcodec-avx2	64B	safe	64	13.76	4.653
url_encode	vcodec-avx2	64B	text	64	34.2	1.871
url_encode	vcodec-avx2	64KiB	binary	65536	2.799e+04	2.342
url_encode	vcodec-avx2	64KiB	pct10	65536	2.688e+04	2.438
url_encode	vcodec-avx2	64KiB	safe	65536	3013	21.75
url_encode	vcodec-avx2	64KiB	text	65536	2.802e+04	2.339
url_encode	vcodec-portable	1KiB	binary	1024	7330	0.1397
url_encode	vcodec-portable	1KiB	pct10	1024	6351	0.1612
url_encode	vcodec-portable	1KiB	safe	1024	1272	0.8051
url_encode	vcodec-portable	1KiB	text	1024	7799	0.1313
url_encode	vcodec-portable	4MiB	binary	4194304	3.85e+07	0.109
url_encode	vcodec-portable	4MiB	pct10	4194304	3.109e+07	0.1349
url_encode	vcodec-portable	4MiB	safe	4194304	5.32e+06	0.7884
url_encode	vcodec-portable	4MiB	text	4194304	3.746e+07	0.112
url_encode	vcodec-portable	64B	binary	64	478.5	0.1337
url_encode	vcodec-portable	64B	pct10	64	277.5	0.2307
url_encode	vcodec-portable	64B	safe	64	89.63	0.7141
url_encode	vcodec-portable	64B	text	64	482.5	0.1326
url_encode	vcodec-portable	64KiB	binary	65536	5.756e+05	0.1139
url_encode	vcodec-portable	64KiB	pct10	65536	4.7e+05	0.1394
url_encode	vcodec-portable	64KiB	safe	65536	7.935e+04	0.8259
url_encode	vcodec-portable	64KiB	text	65536	5.584e+05	0.1174
url_encode	vcodec-scalar	1KiB	binary	1024	1179	0.8687
url_encode	vcodec-scalar	1KiB	pct10	1024	789.9	1.296
url_encode	vcodec-scalar	1KiB	safe	1024	805.7	1.271
url_encode	vcodec-scalar	1KiB	text	1024	865.2	1.184
url_encode	vcodec-scalar	4MiB	binary	4194304	1.811e+07	0.2316
url_encode	vcodec-scalar	4MiB	pct10	4194304	8.931e+06	0.4696
url_encode	vcodec-scalar	4MiB	safe	4194304	3.053e+06	1.374
url_encode	vcodec-scalar	4MiB	text	4194304	1.375e+07	0.305
url_encode	vcodec-scalar	64B	binary	64	87.55	0.731
url_encode	vcodec-scalar	64B	pct10	64	58.87	1.087
url_encode	vcodec-scalar	64B	safe	64	56.85	1.126
url_encode	vcodec-scalar	64B	text	64	73.19	0.8745
url_encode	vcodec-scalar	64KiB	binary	65536	2.669e+05	0.2455
url_encode	vcodec-scalar	64KiB	pct10	65536	1.373e+05	0.4772
url_encode	vcodec-scalar	64KiB	safe	65536	4.764e+04	1.376
url_encode	vcodec-scalar	64KiB	text	65536	2.013e+05	0.3256
url_encode	vcodec-sse4.1	1KiB	binary	1024	450.4	2.274
url_encode	vcodec-sse4.1	1KiB	pct10	1024	378.2	2.707
url_encode	vcodec-sse4.1	1KiB	safe	1024	88.18	11.61
url_encode	vcodec-sse4.1	1KiB	text	1024	450.6	2.273
url_encode	vcodec-sse4.1	4MiB	binary	4194304	1.909e+06	2.198
url_encode	vcodec-sse4.1	4MiB	pct10	4194304	2.056e+06	2.04
url_encode	vcodec-sse4.1	4MiB	safe	4194304	4.016e+05	10.44
url_encode	vcodec-sse4.1	4MiB	text	4194304	1.986e+06	2.112
url_encode	vcodec-sse4.1	64B	binary	64	41.62	1.538
url_encode	vcodec-sse4.1	64B	pct10	64	25.81	2.48
url_encode	vcodec-sse4.1	64B	safe	64	15.02	4.261
url_encode	vcodec-sse4.1	64B	text	64	36.55	1.751
url_encode	vcodec-sse4.1	64KiB	binary	65536	2.818e+04	2.325
url_encode	vcodec-sse4.1	64KiB	pct10	65536	2.306e+04	2.842
url_encode	vcodec-sse4.1	64KiB	safe	65536	5146	12.74
url_encode	vcodec-sse4.1	64KiB	text	65536	2.751e+04	2.382


# Markdown tables (GB/s, higher is better; speedup vs vcodec-scalar in parentheses)


## base64_decode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | base64-crate | base64-simd | openssl-EVP_DecodeBlock |
|---|---|---|---|---|---|---|---|
| 64B random | 1.69 (1.2×) | 0.64 (0.4×) | 1.42 (1.0×) | 2.65 (1.9×) | 2.19 (1.5×) | 5.63 (4.0×) | 2.07 (1.5×) |
| 1KiB random | 11.56 (6.4×) | 0.68 (0.4×) | 1.82 (1.0×) | 8.51 (4.7×) | 2.53 (1.4×) | 9.14 (5.0×) | 2.39 (1.3×) |
| 64KiB random | 16.45 (8.3×) | 0.68 (0.3×) | 1.97 (1.0×) | 8.44 (4.3×) | 2.64 (1.3×) | 9.76 (4.9×) | 2.41 (1.2×) |
| 4MiB random | 14.91 (7.8×) | 0.70 (0.4×) | 1.91 (1.0×) | 9.65 (5.1×) | 2.61 (1.4×) | 8.81 (4.6×) | 2.28 (1.2×) |

## base64_encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | base64-crate | base64-simd | openssl-EVP_EncodeBlock |
|---|---|---|---|---|---|---|---|
| 64B random | 2.24 (2.0×) | 0.45 (0.4×) | 1.12 (1.0×) | 2.77 (2.5×) | 2.03 (1.8×) | 4.96 (4.4×) | 1.62 (1.4×) |
| 1KiB random | 12.28 (9.1×) | 0.50 (0.4×) | 1.34 (1.0×) | 7.68 (5.7×) | 2.67 (2.0×) | 9.57 (7.1×) | 1.92 (1.4×) |
| 64KiB random | 17.41 (11.9×) | 0.46 (0.3×) | 1.46 (1.0×) | 9.06 (6.2×) | 2.60 (1.8×) | 10.03 (6.9×) | 1.93 (1.3×) |
| 4MiB random | 12.49 (8.8×) | 0.49 (0.3×) | 1.41 (1.0×) | 7.62 (5.4×) | 2.67 (1.9×) | 10.28 (7.3×) | 1.94 (1.4×) |

## hex_decode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | hex-crate | hex-simd |
|---|---|---|---|---|---|---|
| 64B random | 9.99 (5.3×) | 2.56 (1.3×) | 1.90 (1.0×) | 6.49 (3.4×) | 0.89 (0.5×) | 22.23 (11.7×) |
| 1KiB random | 12.45 (6.6×) | 2.49 (1.3×) | 1.88 (1.0×) | 6.64 (3.5×) | 1.05 (0.6×) | 21.86 (11.6×) |
| 64KiB random | 13.38 (6.3×) | 2.26 (1.1×) | 2.12 (1.0×) | 6.81 (3.2×) | 0.25 (0.1×) | 20.47 (9.6×) |
| 4MiB random | 13.28 (6.7×) | 2.48 (1.3×) | 1.97 (1.0×) | 7.33 (3.7×) | 0.23 (0.1×) | 17.03 (8.6×) |

## hex_encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | hex-crate | hex-simd |
|---|---|---|---|---|---|---|
| 64B random | 4.36 (3.8×) | 1.25 (1.1×) | 1.14 (1.0×) | 4.11 (3.6×) | 1.05 (0.9×) | 17.20 (15.1×) |
| 1KiB random | 23.66 (17.3×) | 1.45 (1.1×) | 1.37 (1.0×) | 12.22 (8.9×) | 1.16 (0.8×) | 27.53 (20.1×) |
| 64KiB random | 19.30 (13.8×) | 1.23 (0.9×) | 1.39 (1.0×) | 13.79 (9.9×) | 1.48 (1.1×) | 17.76 (12.7×) |
| 4MiB random | 8.71 (6.3×) | 1.28 (0.9×) | 1.38 (1.0×) | 9.70 (7.0×) | 1.33 (1.0×) | 8.88 (6.4×) |

## url_decode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | percent-encoding |
|---|---|---|---|---|---|
| 64B binary | 3.00 (1.7×) | 0.96 (0.5×) | 1.80 (1.0×) | 2.35 (1.3×) | 1.13 (0.6×) |
| 64B pct10 | 2.56 (2.2×) | 1.07 (0.9×) | 1.18 (1.0×) | 2.35 (2.0×) | 0.64 (0.5×) |
| 64B safe | 1.71 (1.3×) | 2.98 (2.2×) | 1.37 (1.0×) | 2.78 (2.0×) | 0.77 (0.6×) |
| 64B text | 2.31 (1.7×) | 0.85 (0.6×) | 1.39 (1.0×) | 2.07 (1.5×) | 0.89 (0.6×) |
| 1KiB binary | 3.11 (2.0×) | 0.94 (0.6×) | 1.56 (1.0×) | 2.29 (1.5×) | 1.34 (0.9×) |
| 1KiB pct10 | 3.33 (2.1×) | 0.99 (0.6×) | 1.56 (1.0×) | 2.79 (1.8×) | 1.43 (0.9×) |
| 1KiB safe | 18.47 (12.0×) | 14.47 (9.4×) | 1.54 (1.0×) | 11.29 (7.3×) | 1.34 (0.9×) |
| 1KiB text | 3.32 (2.3×) | 0.82 (0.6×) | 1.46 (1.0×) | 2.26 (1.5×) | 1.33 (0.9×) |
| 64KiB binary | 3.14 (5.1×) | 0.69 (1.1×) | 0.61 (1.0×) | 2.38 (3.9×) | 0.63 (1.0×) |
| 64KiB pct10 | 3.04 (4.8×) | 0.83 (1.3×) | 0.63 (1.0×) | 2.60 (4.1×) | 0.61 (1.0×) |
| 64KiB safe | 35.24 (22.6×) | 19.34 (12.4×) | 1.56 (1.0×) | 14.67 (9.4×) | 1.32 (0.8×) |
| 64KiB text | 3.02 (6.0×) | 0.68 (1.4×) | 0.50 (1.0×) | 2.28 (4.5×) | 0.52 (1.0×) |
| 4MiB binary | 3.29 (5.6×) | 0.58 (1.0×) | 0.59 (1.0×) | 2.34 (4.0×) | 0.61 (1.0×) |
| 4MiB pct10 | 3.16 (5.6×) | 0.59 (1.1×) | 0.56 (1.0×) | 2.05 (3.6×) | 0.55 (1.0×) |
| 4MiB safe | 14.19 (9.0×) | 13.78 (8.7×) | 1.58 (1.0×) | 12.24 (7.7×) | 1.46 (0.9×) |
| 4MiB text | 3.03 (6.5×) | 0.57 (1.2×) | 0.47 (1.0×) | 2.24 (4.8×) | 0.46 (1.0×) |

## url_encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | percent-encoding |
|---|---|---|---|---|---|
| 64B binary | 1.73 (2.4×) | 0.13 (0.2×) | 0.73 (1.0×) | 1.54 (2.1×) | 0.16 (0.2×) |
| 64B pct10 | 1.57 (1.4×) | 0.23 (0.2×) | 1.09 (1.0×) | 2.48 (2.3×) | 0.32 (0.3×) |
| 64B safe | 4.65 (4.1×) | 0.71 (0.6×) | 1.13 (1.0×) | 4.26 (3.8×) | 1.18 (1.0×) |
| 64B text | 1.87 (2.1×) | 0.13 (0.2×) | 0.87 (1.0×) | 1.75 (2.0×) | 0.25 (0.3×) |
| 1KiB binary | 2.30 (2.6×) | 0.14 (0.2×) | 0.87 (1.0×) | 2.27 (2.6×) | 0.26 (0.3×) |
| 1KiB pct10 | 2.54 (2.0×) | 0.16 (0.1×) | 1.30 (1.0×) | 2.71 (2.1×) | 0.68 (0.5×) |
| 1KiB safe | 21.83 (17.2×) | 0.81 (0.6×) | 1.27 (1.0×) | 11.61 (9.1×) | 1.54 (1.2×) |
| 1KiB text | 2.33 (2.0×) | 0.13 (0.1×) | 1.18 (1.0×) | 2.27 (1.9×) | 0.47 (0.4×) |
| 64KiB binary | 2.34 (9.5×) | 0.11 (0.5×) | 0.25 (1.0×) | 2.33 (9.5×) | 0.11 (0.4×) |
| 64KiB pct10 | 2.44 (5.1×) | 0.14 (0.3×) | 0.48 (1.0×) | 2.84 (6.0×) | 0.49 (1.0×) |
| 64KiB safe | 21.75 (15.8×) | 0.83 (0.6×) | 1.38 (1.0×) | 12.74 (9.3×) | 1.51 (1.1×) |
| 64KiB text | 2.34 (7.2×) | 0.12 (0.4×) | 0.33 (1.0×) | 2.38 (7.3×) | 0.31 (0.9×) |
| 4MiB binary | 2.12 (9.2×) | 0.11 (0.5×) | 0.23 (1.0×) | 2.20 (9.5×) | 0.10 (0.4×) |
| 4MiB pct10 | 2.33 (5.0×) | 0.13 (0.3×) | 0.47 (1.0×) | 2.04 (4.3×) | 0.41 (0.9×) |
| 4MiB safe | 15.04 (10.9×) | 0.79 (0.6×) | 1.37 (1.0×) | 10.44 (7.6×) | 1.36 (1.0×) |
| 4MiB text | 2.33 (7.6×) | 0.11 (0.4×) | 0.30 (1.0×) | 2.11 (6.9×) | 0.27 (0.9×) |
