"""Download and frame the public IPtoASN combined TSV outside the library.

Keep provider bytes and the framed intermediate under an ignored private path.
All source rows, including ASN zero and unknown countries, are retained.
"""
import argparse
import csv
import gzip
import hashlib
import io
import ipaddress
import json
import pathlib
import struct
import urllib.request

p = argparse.ArgumentParser()
p.add_argument('output', type=pathlib.Path)
args = p.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
url = 'https://iptoasn.com/data/ip2asn-combined.tsv.gz'
archive = args.output/'ip2asn-combined.tsv.gz'
request = urllib.request.Request(url, headers={'User-Agent':'netindex-payload-study/1.0'})
with urllib.request.urlopen(request, timeout=120) as response, archive.open('xb') as dest:
    headers = {k:response.headers.get(k) for k in ['Last-Modified','ETag','Content-Length']}
    size = 0
    while block := response.read(1024*1024):
        size += len(block)
        if size > 64*1024*1024:
            raise ValueError('archive exceeds 64 MiB')
        dest.write(block)
digest = hashlib.sha256(b'netindex-payload-stream-v1\0')
records = 0
previous = None
source_bytes = 0
with gzip.open(archive,'rb') as compressed, (args.output/'asn.rows').open('xb') as output:
    text = io.TextIOWrapper(compressed,encoding='utf-8',newline='')
    for row in csv.reader(text,delimiter='\t',quoting=csv.QUOTE_NONE):
        if len(row)!=5:
            raise ValueError('expected exactly five TSV columns')
        first,last,number,country,description = row
        start,end = ipaddress.ip_address(first),ipaddress.ip_address(last)
        if start.version!=end.version or int(start)>int(end):
            raise ValueError('invalid source range')
        current = (start.version,int(start))
        if previous is not None and (current[0]<previous[0] or
                (current[0]==previous[0] and current[1]<=previous[1])):
            raise ValueError('source ranges overlap or are not in family/address order')
        previous = (end.version,int(end))
        # Study-owned framing: version + ASN + country length/country + description.
        # Network endpoints are targets, deliberately excluded from payload values.
        country_bytes = country.encode('utf-8')
        payload = b'\x01'+struct.pack('<IH',int(number),len(country_bytes))+country_bytes+description.encode('utf-8')
        if len(payload)>4096:
            raise ValueError('payload exceeds 4096 bytes')
        output.write(struct.pack('<B',start.version)+int(start).to_bytes(16,'little')+
                     int(end).to_bytes(16,'little')+struct.pack('<I',len(payload))+payload)
        digest.update(struct.pack('<Q',len(payload)))
        digest.update(payload)
        records += 1
        source_bytes += sum(len(field.encode('utf-8')) for field in row)+5
        if records>10_000_000 or source_bytes>1024**3:
            raise ValueError('source exceeds study bounds')
receipt = {'url':url,'license_source':'https://iptoasn.com/','license':'PDDL 1.0',
    'response_headers':headers,'records':records,'normalized_text_bytes':source_bytes,
    'logical_payload_sha256':digest.hexdigest(),
    'files':{name:hashlib.sha256((args.output/name).read_bytes()).hexdigest()
             for name in ['ip2asn-combined.tsv.gz','asn.rows']}}
(args.output/'source.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
