#!/usr/bin/env python3
"""Paste results/summary.md into REPORT.md between the <!-- SUMMARY --> markers."""
import os, re
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPORT = os.path.join(ROOT, 'REPORT-rust.md')
rep = open(REPORT).read()
summ = open(os.path.join(ROOT, 'results', 'summary.md')).read()
block = '<!-- SUMMARY -->\n' + summ.replace('\n## ', '\n### ') .replace('## ', '### ', 1) + '\n<!-- /SUMMARY -->'
if '<!-- /SUMMARY -->' in rep:
    rep = re.sub(r'<!-- SUMMARY -->.*?<!-- /SUMMARY -->', lambda m: block, rep, flags=re.S)
else:
    rep = rep.replace('<!-- SUMMARY -->', block)
open(REPORT, 'w').write(rep)
print('inserted')
