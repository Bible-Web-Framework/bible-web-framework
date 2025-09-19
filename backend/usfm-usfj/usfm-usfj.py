#!/usr/bin/env python

import json
import sys
from glob import glob
from pathlib import Path

from usfm_grammar import USFMParser

if len(sys.argv) == 1:
    print(f"Syntax: {sys.argv[0]} <usfm-input-file(s)>")
    sys.exit(1)

filenames = []
for parameter in sys.argv[1:]:
    globber = glob(parameter)
    filenames.extend(globber)

for input_filename in filenames:
    output_filename = input_filename.rsplit('.', 1)[0] + '.usfj'

    print(f"Converting '{input_filename}' to '{output_filename}'...")
    sys.stdin.reconfigure(encoding='utf-8')
    sys.stdout.reconfigure(encoding='utf-8')

    with open(input_filename, encoding='utf-8') as book_in:
        my_parser = USFMParser(book_in.read())
        scripture_dict: dict = my_parser.to_usj(ignore_errors=True)

    json_output = json.dumps(scripture_dict)

    with open(output_filename, 'w') as book_out:
        book_out.write(json_output)
