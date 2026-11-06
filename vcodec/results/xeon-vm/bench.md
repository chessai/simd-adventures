group	impl	size	dist	bytes	ns	GB/s
base64_decode	base64-crate	1KiB	random	1368	518.7	2.638
base64_decode	base64-crate	4MiB	random	5592408	2.368e+06	2.362
base64_decode	base64-crate	64B	random	88	46.33	1.9
base64_decode	base64-crate	64KiB	random	87384	3.362e+04	2.599
base64_decode	base64-simd	1KiB	random	1368	147.8	9.255
base64_decode	base64-simd	4MiB	random	5592408	6.205e+05	9.012
base64_decode	base64-simd	64B	random	88	16.83	5.228
base64_decode	base64-simd	64KiB	random	87384	9000	9.709
base64_decode	openssl-EVP_DecodeBlock	1KiB	random	1368	602.4	2.271
base64_decode	openssl-EVP_DecodeBlock	4MiB	random	5592408	2.368e+06	2.362
base64_decode	openssl-EVP_DecodeBlock	64B	random	88	43.51	2.022
base64_decode	openssl-EVP_DecodeBlock	64KiB	random	87384	4.154e+04	2.104
base64_decode	vcodec-avx2	1KiB	random	1368	155	8.824
base64_decode	vcodec-avx2	4MiB	random	5592408	3.932e+05	14.22
base64_decode	vcodec-avx2	64B	random	88	57.98	1.518
base64_decode	vcodec-avx2	64KiB	random	87384	6502	13.44
base64_decode	vcodec-portable	1KiB	random	1368	2262	0.6047
base64_decode	vcodec-portable	4MiB	random	5592408	8.538e+06	0.655
base64_decode	vcodec-portable	64B	random	88	132.5	0.664
base64_decode	vcodec-portable	64KiB	random	87384	1.331e+05	0.6567
base64_decode	vcodec-scalar	1KiB	random	1368	730.1	1.874
base64_decode	vcodec-scalar	4MiB	random	5592408	3.102e+06	1.803
base64_decode	vcodec-scalar	64B	random	88	59.54	1.478
base64_decode	vcodec-scalar	64KiB	random	87384	4.824e+04	1.811
base64_decode	vcodec-sse4.1	1KiB	random	1368	176.5	7.75
base64_decode	vcodec-sse4.1	4MiB	random	5592408	6.715e+05	8.328
base64_decode	vcodec-sse4.1	64B	random	88	33.99	2.589
base64_decode	vcodec-sse4.1	64KiB	random	87384	9271	9.426
base64_encode	base64-crate	1KiB	random	1024	412.2	2.484
base64_encode	base64-crate	4MiB	random	4194304	1.682e+06	2.494
base64_encode	base64-crate	64B	random	64	32.12	1.992
base64_encode	base64-crate	64KiB	random	65536	2.543e+04	2.577
base64_encode	base64-simd	1KiB	random	1024	106.2	9.641
base64_encode	base64-simd	4MiB	random	4194304	4.325e+05	9.698
base64_encode	base64-simd	64B	random	64	12.8	5
base64_encode	base64-simd	64KiB	random	65536	6804	9.632
base64_encode	openssl-EVP_EncodeBlock	1KiB	random	1024	547	1.872
base64_encode	openssl-EVP_EncodeBlock	4MiB	random	4194304	2.149e+06	1.952
base64_encode	openssl-EVP_EncodeBlock	64B	random	64	39.29	1.629
base64_encode	openssl-EVP_EncodeBlock	64KiB	random	65536	3.365e+04	1.948
base64_encode	vcodec-avx2	1KiB	random	1024	87.16	11.75
base64_encode	vcodec-avx2	4MiB	random	4194304	3.74e+05	11.21
base64_encode	vcodec-avx2	64B	random	64	28.48	2.247
base64_encode	vcodec-avx2	64KiB	random	65536	4768	13.74
base64_encode	vcodec-portable	1KiB	random	1024	2150	0.4763
base64_encode	vcodec-portable	4MiB	random	4194304	1.026e+07	0.409
base64_encode	vcodec-portable	64B	random	64	138.8	0.461
base64_encode	vcodec-portable	64KiB	random	65536	1.448e+05	0.4527
base64_encode	vcodec-scalar	1KiB	random	1024	730.4	1.402
base64_encode	vcodec-scalar	4MiB	random	4194304	2.981e+06	1.407
base64_encode	vcodec-scalar	64B	random	64	53.65	1.193
base64_encode	vcodec-scalar	64KiB	random	65536	4.45e+04	1.473
base64_encode	vcodec-sse4.1	1KiB	random	1024	162.6	6.299
base64_encode	vcodec-sse4.1	4MiB	random	4194304	5.345e+05	7.847
base64_encode	vcodec-sse4.1	64B	random	64	23.18	2.761
base64_encode	vcodec-sse4.1	64KiB	random	65536	7415	8.838
hex_decode	hex-crate	1KiB	random	2048	2248	0.9109
hex_decode	hex-crate	4MiB	random	8388608	3.683e+07	0.2277
hex_decode	hex-crate	64B	random	128	145.8	0.8781
hex_decode	hex-crate	64KiB	random	131072	6.015e+05	0.2179
hex_decode	hex-simd	1KiB	random	2048	93.07	22
hex_decode	hex-simd	4MiB	random	8388608	4.493e+05	18.67
hex_decode	hex-simd	64B	random	128	6.273	20.4
hex_decode	hex-simd	64KiB	random	131072	7334	17.87
hex_decode	vcodec-avx2	1KiB	random	2048	91.94	22.27
hex_decode	vcodec-avx2	4MiB	random	8388608	4.489e+05	18.69
hex_decode	vcodec-avx2	64B	random	128	10.36	12.35
hex_decode	vcodec-avx2	64KiB	random	131072	6592	19.88
hex_decode	vcodec-avx2-rangecheck	1KiB	random	2048	161.7	12.66
hex_decode	vcodec-avx2-rangecheck	4MiB	random	8388608	6.55e+05	12.81
hex_decode	vcodec-avx2-rangecheck	64B	random	128	13.06	9.799
hex_decode	vcodec-avx2-rangecheck	64KiB	random	131072	1.078e+04	12.16
hex_decode	vcodec-portable	1KiB	random	2048	2149	0.953
hex_decode	vcodec-portable	4MiB	random	8388608	1.038e+07	0.8084
hex_decode	vcodec-portable	64B	random	128	146	0.8765
hex_decode	vcodec-portable	64KiB	random	131072	1.358e+05	0.9649
hex_decode	vcodec-portable-rangecheck	1KiB	random	2048	977.6	2.095
hex_decode	vcodec-portable-rangecheck	4MiB	random	8388608	3.373e+06	2.487
hex_decode	vcodec-portable-rangecheck	64B	random	128	54.55	2.346
hex_decode	vcodec-portable-rangecheck	64KiB	random	131072	5.316e+04	2.465
hex_decode	vcodec-scalar	1KiB	random	2048	971.1	2.109
hex_decode	vcodec-scalar	4MiB	random	8388608	4.179e+06	2.007
hex_decode	vcodec-scalar	64B	random	128	65.53	1.953
hex_decode	vcodec-scalar	64KiB	random	131072	6.092e+04	2.151
hex_decode	vcodec-sse4.1	1KiB	random	2048	171.5	11.95
hex_decode	vcodec-sse4.1	4MiB	random	8388608	6.73e+05	12.46
hex_decode	vcodec-sse4.1	64B	random	128	15.28	8.379
hex_decode	vcodec-sse4.1	64KiB	random	131072	1.101e+04	11.9
hex_decode	vcodec-sse4.1-rangecheck	1KiB	random	2048	283.3	7.229
hex_decode	vcodec-sse4.1-rangecheck	4MiB	random	8388608	1.122e+06	7.474
hex_decode	vcodec-sse4.1-rangecheck	64B	random	128	21.09	6.068
hex_decode	vcodec-sse4.1-rangecheck	64KiB	random	131072	1.831e+04	7.158
hex_encode	hex-crate	1KiB	random	1024	731.5	1.4
hex_encode	hex-crate	4MiB	random	4194304	3.214e+06	1.305
hex_encode	hex-crate	64B	random	64	47.61	1.344
hex_encode	hex-crate	64KiB	random	65536	4.467e+04	1.467
hex_encode	hex-simd	1KiB	random	1024	35.6	28.76
hex_encode	hex-simd	4MiB	random	4194304	4.842e+05	8.663
hex_encode	hex-simd	64B	random	64	4.189	15.28
hex_encode	hex-simd	64KiB	random	65536	3448	19.01
hex_encode	vcodec-avx2	1KiB	random	1024	47.41	21.6
hex_encode	vcodec-avx2	4MiB	random	4194304	4.78e+05	8.774
hex_encode	vcodec-avx2	64B	random	64	14.93	4.286
hex_encode	vcodec-avx2	64KiB	random	65536	3566	18.38
hex_encode	vcodec-portable	1KiB	random	1024	732	1.399
hex_encode	vcodec-portable	4MiB	random	4194304	3.393e+06	1.236
hex_encode	vcodec-portable	64B	random	64	58.18	1.1
hex_encode	vcodec-portable	64KiB	random	65536	5.75e+04	1.14
hex_encode	vcodec-scalar	1KiB	random	1024	765.4	1.338
hex_encode	vcodec-scalar	4MiB	random	4194304	3.218e+06	1.304
hex_encode	vcodec-scalar	64B	random	64	59.33	1.079
hex_encode	vcodec-scalar	64KiB	random	65536	4.968e+04	1.319
hex_encode	vcodec-sse4.1	1KiB	random	1024	97.2	10.54
hex_encode	vcodec-sse4.1	4MiB	random	4194304	4.362e+05	9.616
hex_encode	vcodec-sse4.1	64B	random	64	15.4	4.156
hex_encode	vcodec-sse4.1	64KiB	random	65536	4752	13.79
url_decode	percent-encoding	1KiB	binary	2610	2185	1.195
url_decode	percent-encoding	1KiB	pct10	1216	1132	1.074
url_decode	percent-encoding	1KiB	safe	1024	1098	0.933
url_decode	percent-encoding	1KiB	text	1360	1238	1.099
url_decode	percent-encoding	4MiB	binary	10422876	1.74e+07	0.5991
url_decode	percent-encoding	4MiB	pct10	5032344	8.548e+06	0.5887
url_decode	percent-encoding	4MiB	safe	4194304	4.247e+06	0.9875
url_decode	percent-encoding	4MiB	text	5682444	1.031e+07	0.551
url_decode	percent-encoding	64B	binary	178	166.2	1.071
url_decode	percent-encoding	64B	pct10	76	112.2	0.6776
url_decode	percent-encoding	64B	safe	64	107.2	0.5969
url_decode	percent-encoding	64B	text	88	111.5	0.7894
url_decode	percent-encoding	64KiB	binary	162804	2.701e+05	0.6028
url_decode	percent-encoding	64KiB	pct10	78566	1.234e+05	0.6369
url_decode	percent-encoding	64KiB	safe	65536	6.161e+04	1.064
url_decode	percent-encoding	64KiB	text	88676	1.555e+05	0.5704
url_decode	vcodec-avx2	1KiB	binary	2610	843.3	3.095
url_decode	vcodec-avx2	1KiB	pct10	1216	413	2.944
url_decode	vcodec-avx2	1KiB	safe	1024	58.31	17.56
url_decode	vcodec-avx2	1KiB	text	1360	458.6	2.965
url_decode	vcodec-avx2	4MiB	binary	10422876	3.315e+06	3.144
url_decode	vcodec-avx2	4MiB	pct10	5032344	1.682e+06	2.991
url_decode	vcodec-avx2	4MiB	safe	4194304	3.002e+05	13.97
url_decode	vcodec-avx2	4MiB	text	5682444	1.834e+06	3.099
url_decode	vcodec-avx2	64B	binary	178	65.11	2.734
url_decode	vcodec-avx2	64B	pct10	76	32.73	2.322
url_decode	vcodec-avx2	64B	safe	64	37.06	1.727
url_decode	vcodec-avx2	64B	text	88	38.44	2.289
url_decode	vcodec-avx2	64KiB	binary	162804	5.282e+04	3.082
url_decode	vcodec-avx2	64KiB	pct10	78566	2.407e+04	3.263
url_decode	vcodec-avx2	64KiB	safe	65536	1869	35.06
url_decode	vcodec-avx2	64KiB	text	88676	3.364e+04	2.636
url_decode	vcodec-portable	1KiB	binary	2610	7555	0.3455
url_decode	vcodec-portable	1KiB	pct10	1216	3061	0.3973
url_decode	vcodec-portable	1KiB	safe	1024	692.5	1.479
url_decode	vcodec-portable	1KiB	text	1360	4328	0.3142
url_decode	vcodec-portable	4MiB	binary	10422876	3.907e+07	0.2668
url_decode	vcodec-portable	4MiB	pct10	5032344	1.68e+07	0.2996
url_decode	vcodec-portable	4MiB	safe	4194304	3.267e+06	1.284
url_decode	vcodec-portable	4MiB	text	5682444	1.975e+07	0.2878
url_decode	vcodec-portable	64B	binary	178	545.8	0.3262
url_decode	vcodec-portable	64B	pct10	76	174.6	0.4354
url_decode	vcodec-portable	64B	safe	64	52.35	1.223
url_decode	vcodec-portable	64B	text	88	283.7	0.3102
url_decode	vcodec-portable	64KiB	binary	162804	5.086e+05	0.3201
url_decode	vcodec-portable	64KiB	pct10	78566	2.321e+05	0.3385
url_decode	vcodec-portable	64KiB	safe	65536	4.562e+04	1.437
url_decode	vcodec-portable	64KiB	text	88676	2.857e+05	0.3104
url_decode	vcodec-scalar	1KiB	binary	2610	1774	1.472
url_decode	vcodec-scalar	1KiB	pct10	1216	1109	1.097
url_decode	vcodec-scalar	1KiB	safe	1024	1008	1.016
url_decode	vcodec-scalar	1KiB	text	1360	1155	1.178
url_decode	vcodec-scalar	4MiB	binary	10422876	1.675e+07	0.6224
url_decode	vcodec-scalar	4MiB	pct10	5032344	7.506e+06	0.6704
url_decode	vcodec-scalar	4MiB	safe	4194304	5.391e+06	0.7781
url_decode	vcodec-scalar	4MiB	text	5682444	9.628e+06	0.5902
url_decode	vcodec-scalar	64B	binary	178	124.8	1.427
url_decode	vcodec-scalar	64B	pct10	76	75.13	1.012
url_decode	vcodec-scalar	64B	safe	64	68.9	0.9289
url_decode	vcodec-scalar	64B	text	88	77.85	1.13
url_decode	vcodec-scalar	64KiB	binary	162804	2.361e+05	0.6896
url_decode	vcodec-scalar	64KiB	pct10	78566	1.118e+05	0.7028
url_decode	vcodec-scalar	64KiB	safe	65536	6.716e+04	0.9758
url_decode	vcodec-scalar	64KiB	text	88676	1.421e+05	0.6242
url_decode	vcodec-sse4.1	1KiB	binary	2610	1346	1.939
url_decode	vcodec-sse4.1	1KiB	pct10	1216	523	2.325
url_decode	vcodec-sse4.1	1KiB	safe	1024	116.6	8.784
url_decode	vcodec-sse4.1	1KiB	text	1360	724.5	1.877
url_decode	vcodec-sse4.1	4MiB	binary	10422876	5.178e+06	2.013
url_decode	vcodec-sse4.1	4MiB	pct10	5032344	2.982e+06	1.687
url_decode	vcodec-sse4.1	4MiB	safe	4194304	3.748e+05	11.19
url_decode	vcodec-sse4.1	4MiB	text	5682444	2.795e+06	2.033
url_decode	vcodec-sse4.1	64B	binary	178	103.4	1.721
url_decode	vcodec-sse4.1	64B	pct10	76	35.68	2.13
url_decode	vcodec-sse4.1	64B	safe	64	23.84	2.685
url_decode	vcodec-sse4.1	64B	text	88	46	1.913
url_decode	vcodec-sse4.1	64KiB	binary	162804	8.133e+04	2.002
url_decode	vcodec-sse4.1	64KiB	pct10	78566	3.293e+04	2.386
url_decode	vcodec-sse4.1	64KiB	safe	65536	6715	9.76
url_decode	vcodec-sse4.1	64KiB	text	88676	4.58e+04	1.936
url_encode	percent-encoding	1KiB	binary	1024	4498	0.2277
url_encode	percent-encoding	1KiB	pct10	1024	1458	0.7023
url_encode	percent-encoding	1KiB	safe	1024	727.2	1.408
url_encode	percent-encoding	1KiB	text	1024	1892	0.5412
url_encode	percent-encoding	4MiB	binary	4194304	4.121e+07	0.1018
url_encode	percent-encoding	4MiB	pct10	4194304	1.023e+07	0.4098
url_encode	percent-encoding	4MiB	safe	4194304	2.879e+06	1.457
url_encode	percent-encoding	4MiB	text	4194304	1.512e+07	0.2773
url_encode	percent-encoding	64B	binary	64	367.1	0.1743
url_encode	percent-encoding	64B	pct10	64	250.8	0.2552
url_encode	percent-encoding	64B	safe	64	54.67	1.171
url_encode	percent-encoding	64B	text	64	250.8	0.2551
url_encode	percent-encoding	64KiB	binary	65536	6.17e+05	0.1062
url_encode	percent-encoding	64KiB	pct10	65536	1.286e+05	0.5096
url_encode	percent-encoding	64KiB	safe	65536	4.467e+04	1.467
url_encode	percent-encoding	64KiB	text	65536	2.141e+05	0.3061
url_encode	vcodec-avx2	1KiB	binary	1024	422.4	2.424
url_encode	vcodec-avx2	1KiB	pct10	1024	422.4	2.424
url_encode	vcodec-avx2	1KiB	safe	1024	47.94	21.36
url_encode	vcodec-avx2	1KiB	text	1024	421.4	2.43
url_encode	vcodec-avx2	4MiB	binary	4194304	1.89e+06	2.22
url_encode	vcodec-avx2	4MiB	pct10	4194304	1.784e+06	2.351
url_encode	vcodec-avx2	4MiB	safe	4194304	2.909e+05	14.42
url_encode	vcodec-avx2	4MiB	text	4194304	1.785e+06	2.35
url_encode	vcodec-avx2	64B	binary	64	37.02	1.729
url_encode	vcodec-avx2	64B	pct10	64	33.91	1.888
url_encode	vcodec-avx2	64B	safe	64	13.88	4.611
url_encode	vcodec-avx2	64B	text	64	35.78	1.789
url_encode	vcodec-avx2	64KiB	binary	65536	2.691e+04	2.436
url_encode	vcodec-avx2	64KiB	pct10	65536	2.714e+04	2.415
url_encode	vcodec-avx2	64KiB	safe	65536	2843	23.05
url_encode	vcodec-avx2	64KiB	text	65536	3.246e+04	2.019
url_encode	vcodec-portable	1KiB	binary	1024	7822	0.1309
url_encode	vcodec-portable	1KiB	pct10	1024	6743	0.1519
url_encode	vcodec-portable	1KiB	safe	1024	1267	0.8085
url_encode	vcodec-portable	1KiB	text	1024	7987	0.1282
url_encode	vcodec-portable	4MiB	binary	4194304	3.896e+07	0.1077
url_encode	vcodec-portable	4MiB	pct10	4194304	2.991e+07	0.1402
url_encode	vcodec-portable	4MiB	safe	4194304	5.513e+06	0.7608
url_encode	vcodec-portable	4MiB	text	4194304	3.781e+07	0.1109
url_encode	vcodec-portable	64B	binary	64	539.1	0.1187
url_encode	vcodec-portable	64B	pct10	64	306.6	0.2087
url_encode	vcodec-portable	64B	safe	64	89.76	0.713
url_encode	vcodec-portable	64B	text	64	490	0.1306
url_encode	vcodec-portable	64KiB	binary	65536	5.726e+05	0.1144
url_encode	vcodec-portable	64KiB	pct10	65536	4.717e+05	0.1389
url_encode	vcodec-portable	64KiB	safe	65536	9.166e+04	0.715
url_encode	vcodec-portable	64KiB	text	65536	5.615e+05	0.1167
url_encode	vcodec-scalar	1KiB	binary	1024	1254	0.8169
url_encode	vcodec-scalar	1KiB	pct10	1024	1024	1
url_encode	vcodec-scalar	1KiB	safe	1024	920.2	1.113
url_encode	vcodec-scalar	1KiB	text	1024	1115	0.9188
url_encode	vcodec-scalar	4MiB	binary	4194304	1.898e+07	0.221
url_encode	vcodec-scalar	4MiB	pct10	4194304	9.493e+06	0.4418
url_encode	vcodec-scalar	4MiB	safe	4194304	3.916e+06	1.071
url_encode	vcodec-scalar	4MiB	text	4194304	1.291e+07	0.3249
url_encode	vcodec-scalar	64B	binary	64	84.85	0.7543
url_encode	vcodec-scalar	64B	pct10	64	76.58	0.8358
url_encode	vcodec-scalar	64B	safe	64	69.67	0.9186
url_encode	vcodec-scalar	64B	text	64	87.98	0.7275
url_encode	vcodec-scalar	64KiB	binary	65536	2.683e+05	0.2442
url_encode	vcodec-scalar	64KiB	pct10	65536	1.367e+05	0.4795
url_encode	vcodec-scalar	64KiB	safe	65536	6.175e+04	1.061
url_encode	vcodec-scalar	64KiB	text	65536	1.844e+05	0.3554
url_encode	vcodec-sse4.1	1KiB	binary	1024	460.4	2.224
url_encode	vcodec-sse4.1	1KiB	pct10	1024	412.2	2.484
url_encode	vcodec-sse4.1	1KiB	safe	1024	94.28	10.86
url_encode	vcodec-sse4.1	1KiB	text	1024	436.6	2.345
url_encode	vcodec-sse4.1	4MiB	binary	4194304	2.04e+06	2.056
url_encode	vcodec-sse4.1	4MiB	pct10	4194304	2.01e+06	2.087
url_encode	vcodec-sse4.1	4MiB	safe	4194304	3.341e+05	12.55
url_encode	vcodec-sse4.1	4MiB	text	4194304	1.901e+06	2.206
url_encode	vcodec-sse4.1	64B	binary	64	37.68	1.698
url_encode	vcodec-sse4.1	64B	pct10	64	25.4	2.519
url_encode	vcodec-sse4.1	64B	safe	64	16.04	3.99
url_encode	vcodec-sse4.1	64B	text	64	39.92	1.603
url_encode	vcodec-sse4.1	64KiB	binary	65536	3.114e+04	2.104
url_encode	vcodec-sse4.1	64KiB	pct10	65536	2.422e+04	2.706
url_encode	vcodec-sse4.1	64KiB	safe	65536	5332	12.29
url_encode	vcodec-sse4.1	64KiB	text	65536	2.949e+04	2.222


# Markdown tables (GB/s, higher is better; speedup vs vcodec-scalar in parentheses)


## base64_decode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | base64-crate | base64-simd | openssl-EVP_DecodeBlock |
|---|---|---|---|---|---|---|---|
| 64B random | 1.52 (1.0×) | 0.66 (0.4×) | 1.48 (1.0×) | 2.59 (1.8×) | 1.90 (1.3×) | 5.23 (3.5×) | 2.02 (1.4×) |
| 1KiB random | 8.82 (4.7×) | 0.60 (0.3×) | 1.87 (1.0×) | 7.75 (4.1×) | 2.64 (1.4×) | 9.25 (4.9×) | 2.27 (1.2×) |
| 64KiB random | 13.44 (7.4×) | 0.66 (0.4×) | 1.81 (1.0×) | 9.43 (5.2×) | 2.60 (1.4×) | 9.71 (5.4×) | 2.10 (1.2×) |
| 4MiB random | 14.22 (7.9×) | 0.66 (0.4×) | 1.80 (1.0×) | 8.33 (4.6×) | 2.36 (1.3×) | 9.01 (5.0×) | 2.36 (1.3×) |

## base64_encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | base64-crate | base64-simd | openssl-EVP_EncodeBlock |
|---|---|---|---|---|---|---|---|
| 64B random | 2.25 (1.9×) | 0.46 (0.4×) | 1.19 (1.0×) | 2.76 (2.3×) | 1.99 (1.7×) | 5.00 (4.2×) | 1.63 (1.4×) |
| 1KiB random | 11.75 (8.4×) | 0.48 (0.3×) | 1.40 (1.0×) | 6.30 (4.5×) | 2.48 (1.8×) | 9.64 (6.9×) | 1.87 (1.3×) |
| 64KiB random | 13.74 (9.3×) | 0.45 (0.3×) | 1.47 (1.0×) | 8.84 (6.0×) | 2.58 (1.7×) | 9.63 (6.5×) | 1.95 (1.3×) |
| 4MiB random | 11.21 (8.0×) | 0.41 (0.3×) | 1.41 (1.0×) | 7.85 (5.6×) | 2.49 (1.8×) | 9.70 (6.9×) | 1.95 (1.4×) |

## hex_decode

| size/dist | vcodec-avx2 | vcodec-avx2-rangecheck | vcodec-portable | vcodec-portable-rangecheck | vcodec-scalar | vcodec-sse4.1 | vcodec-sse4.1-rangecheck | hex-crate | hex-simd |
|---|---|---|---|---|---|---|---|---|---|
| 64B random | 12.35 (6.3×) | 9.80 (5.0×) | 0.88 (0.4×) | 2.35 (1.2×) | 1.95 (1.0×) | 8.38 (4.3×) | 6.07 (3.1×) | 0.88 (0.4×) | 20.40 (10.4×) |
| 1KiB random | 22.27 (10.6×) | 12.66 (6.0×) | 0.95 (0.5×) | 2.09 (1.0×) | 2.11 (1.0×) | 11.95 (5.7×) | 7.23 (3.4×) | 0.91 (0.4×) | 22.00 (10.4×) |
| 64KiB random | 19.88 (9.2×) | 12.16 (5.7×) | 0.96 (0.4×) | 2.47 (1.1×) | 2.15 (1.0×) | 11.90 (5.5×) | 7.16 (3.3×) | 0.22 (0.1×) | 17.87 (8.3×) |
| 4MiB random | 18.69 (9.3×) | 12.81 (6.4×) | 0.81 (0.4×) | 2.49 (1.2×) | 2.01 (1.0×) | 12.46 (6.2×) | 7.47 (3.7×) | 0.23 (0.1×) | 18.67 (9.3×) |

## hex_encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | hex-crate | hex-simd |
|---|---|---|---|---|---|---|
| 64B random | 4.29 (4.0×) | 1.10 (1.0×) | 1.08 (1.0×) | 4.16 (3.9×) | 1.34 (1.2×) | 15.28 (14.2×) |
| 1KiB random | 21.60 (16.1×) | 1.40 (1.0×) | 1.34 (1.0×) | 10.54 (7.9×) | 1.40 (1.0×) | 28.76 (21.5×) |
| 64KiB random | 18.38 (13.9×) | 1.14 (0.9×) | 1.32 (1.0×) | 13.79 (10.5×) | 1.47 (1.1×) | 19.01 (14.4×) |
| 4MiB random | 8.77 (6.7×) | 1.24 (0.9×) | 1.30 (1.0×) | 9.62 (7.4×) | 1.31 (1.0×) | 8.66 (6.6×) |

## url_decode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | percent-encoding |
|---|---|---|---|---|---|
| 64B binary | 2.73 (1.9×) | 0.33 (0.2×) | 1.43 (1.0×) | 1.72 (1.2×) | 1.07 (0.8×) |
| 64B pct10 | 2.32 (2.3×) | 0.44 (0.4×) | 1.01 (1.0×) | 2.13 (2.1×) | 0.68 (0.7×) |
| 64B safe | 1.73 (1.9×) | 1.22 (1.3×) | 0.93 (1.0×) | 2.68 (2.9×) | 0.60 (0.6×) |
| 64B text | 2.29 (2.0×) | 0.31 (0.3×) | 1.13 (1.0×) | 1.91 (1.7×) | 0.79 (0.7×) |
| 1KiB binary | 3.09 (2.1×) | 0.35 (0.2×) | 1.47 (1.0×) | 1.94 (1.3×) | 1.19 (0.8×) |
| 1KiB pct10 | 2.94 (2.7×) | 0.40 (0.4×) | 1.10 (1.0×) | 2.33 (2.1×) | 1.07 (1.0×) |
| 1KiB safe | 17.56 (17.3×) | 1.48 (1.5×) | 1.02 (1.0×) | 8.78 (8.6×) | 0.93 (0.9×) |
| 1KiB text | 2.97 (2.5×) | 0.31 (0.3×) | 1.18 (1.0×) | 1.88 (1.6×) | 1.10 (0.9×) |
| 64KiB binary | 3.08 (4.5×) | 0.32 (0.5×) | 0.69 (1.0×) | 2.00 (2.9×) | 0.60 (0.9×) |
| 64KiB pct10 | 3.26 (4.6×) | 0.34 (0.5×) | 0.70 (1.0×) | 2.39 (3.4×) | 0.64 (0.9×) |
| 64KiB safe | 35.06 (35.9×) | 1.44 (1.5×) | 0.98 (1.0×) | 9.76 (10.0×) | 1.06 (1.1×) |
| 64KiB text | 2.64 (4.2×) | 0.31 (0.5×) | 0.62 (1.0×) | 1.94 (3.1×) | 0.57 (0.9×) |
| 4MiB binary | 3.14 (5.1×) | 0.27 (0.4×) | 0.62 (1.0×) | 2.01 (3.2×) | 0.60 (1.0×) |
| 4MiB pct10 | 2.99 (4.5×) | 0.30 (0.4×) | 0.67 (1.0×) | 1.69 (2.5×) | 0.59 (0.9×) |
| 4MiB safe | 13.97 (18.0×) | 1.28 (1.7×) | 0.78 (1.0×) | 11.19 (14.4×) | 0.99 (1.3×) |
| 4MiB text | 3.10 (5.3×) | 0.29 (0.5×) | 0.59 (1.0×) | 2.03 (3.4×) | 0.55 (0.9×) |

## url_encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | percent-encoding |
|---|---|---|---|---|---|
| 64B binary | 1.73 (2.3×) | 0.12 (0.2×) | 0.75 (1.0×) | 1.70 (2.3×) | 0.17 (0.2×) |
| 64B pct10 | 1.89 (2.3×) | 0.21 (0.2×) | 0.84 (1.0×) | 2.52 (3.0×) | 0.26 (0.3×) |
| 64B safe | 4.61 (5.0×) | 0.71 (0.8×) | 0.92 (1.0×) | 3.99 (4.3×) | 1.17 (1.3×) |
| 64B text | 1.79 (2.5×) | 0.13 (0.2×) | 0.73 (1.0×) | 1.60 (2.2×) | 0.26 (0.4×) |
| 1KiB binary | 2.42 (3.0×) | 0.13 (0.2×) | 0.82 (1.0×) | 2.22 (2.7×) | 0.23 (0.3×) |
| 1KiB pct10 | 2.42 (2.4×) | 0.15 (0.2×) | 1.00 (1.0×) | 2.48 (2.5×) | 0.70 (0.7×) |
| 1KiB safe | 21.36 (19.2×) | 0.81 (0.7×) | 1.11 (1.0×) | 10.86 (9.8×) | 1.41 (1.3×) |
| 1KiB text | 2.43 (2.6×) | 0.13 (0.1×) | 0.92 (1.0×) | 2.35 (2.6×) | 0.54 (0.6×) |
| 64KiB binary | 2.44 (10.0×) | 0.11 (0.5×) | 0.24 (1.0×) | 2.10 (8.6×) | 0.11 (0.4×) |
| 64KiB pct10 | 2.42 (5.0×) | 0.14 (0.3×) | 0.48 (1.0×) | 2.71 (5.6×) | 0.51 (1.1×) |
| 64KiB safe | 23.05 (21.7×) | 0.71 (0.7×) | 1.06 (1.0×) | 12.29 (11.6×) | 1.47 (1.4×) |
| 64KiB text | 2.02 (5.7×) | 0.12 (0.3×) | 0.36 (1.0×) | 2.22 (6.3×) | 0.31 (0.9×) |
| 4MiB binary | 2.22 (10.0×) | 0.11 (0.5×) | 0.22 (1.0×) | 2.06 (9.3×) | 0.10 (0.5×) |
| 4MiB pct10 | 2.35 (5.3×) | 0.14 (0.3×) | 0.44 (1.0×) | 2.09 (4.7×) | 0.41 (0.9×) |
| 4MiB safe | 14.42 (13.5×) | 0.76 (0.7×) | 1.07 (1.0×) | 12.55 (11.7×) | 1.46 (1.4×) |
| 4MiB text | 2.35 (7.2×) | 0.11 (0.3×) | 0.32 (1.0×) | 2.21 (6.8×) | 0.28 (0.9×) |
