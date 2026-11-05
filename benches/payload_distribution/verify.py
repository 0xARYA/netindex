"""Verify every retained file, exact accounting and fresh-process repeat parity."""
import argparse
import hashlib
import json
import math
import pathlib

p=argparse.ArgumentParser()
p.add_argument('directory',type=pathlib.Path)
p.add_argument('--output',type=pathlib.Path,required=True)
args=p.parse_args()
root=args.directory
receipt=json.loads((root/'receipt.json').read_text())
def sha(path):
    digest=hashlib.sha256()
    with path.open('rb') as source:
        while block:=source.read(1024*1024): digest.update(block)
    return digest.hexdigest()
for name,expected in receipt['files'].items():
    assert sha(root/name)==expected,name
def quantile(rows,weight,q):
    goal=math.ceil(sum(row[weight] for row in rows)*q)
    total=0
    for row in rows:
        total+=row[weight]
        if total>=goal:return row
    return None

summary={}
for case in ['city-original','city-current','asn']:
    rows=[json.loads((root/f'{case}-r{repeat}.log').read_text()) for repeat in range(2)]
    def semantic(row):return {key:value for key,value in row.items() if key not in ['validation_ms','analysis_total_ms']}
    assert semantic(rows[0])==semantic(rows[1]),case
    result=rows[0]
    counts=result['payloads']
    records=result['records']
    assert result['validated_and_crosschecked_records']==records
    assert sum(row['references'] for row in counts['lengths'])==records
    assert sum(row['distinct'] for row in counts['lengths'])==counts['distinct_values']
    assert sum(row['physical_spans'] for row in counts['lengths'])==counts['physical_nonempty_spans']
    for field,multiplier in [('logical_bytes','references'),('unique_bytes','distinct')]:
        assert sum(row['length']*row[multiplier] for row in counts['lengths'])==counts[field]
    assert sum(row['length']*row['physical_spans'] for row in counts['lengths'])==result['physical_layout']['pool_bytes']
    assert sum(int(freq)*number for freq,number in counts['frequency_histogram'].items())==records
    assert sum(counts['frequency_histogram'].values())==counts['distinct_values']
    assert sum(counts['family_unique_value_masks'].values())==counts['distinct_values']
    assert sum(count for count,_ in counts['family_references_and_logical_bytes'])==records
    assert sum(size for _,size in counts['family_references_and_logical_bytes'])==counts['logical_bytes']
    assert sum(result['physical_layout'][key] for key in ['header_bytes','metadata_bytes','index_bytes','reference_bytes','pool_bytes'])==result['input_bytes']
    reuse=result['insertion_order_reuse']
    assert reuse['cold']+reuse['repeats']==records
    for key in ['reference_gap_histogram','distinct_intervening_histogram','distinct_byte_footprint_histogram']:
        assert sum(row['count'] for row in reuse[key])==reuse['repeats']
    assert all(a<=b for a,b in zip([count for _,count in sorted(reuse['within_byte_budget'].items(),key=lambda x:int(x[0]))],
                                 [count for _,count in sorted(reuse['within_byte_budget'].items(),key=lambda x:int(x[0]))][1:]))
    assert result['modeled_workspace_bytes']<=result['workspace_limit_bytes']
    result['length_quantiles']={weight:{str(q):quantile(counts['lengths'],weight,q)['length']
        for q in [0.5,0.9,0.99,1.0]} for weight in ['references','distinct']}
    result['reuse_quantile_bounds']={key:{str(q):quantile(reuse[key],'count',q) for q in [0.5,0.9,0.99,1.0]}
        for key in ['reference_gap_histogram','distinct_intervening_histogram','distinct_byte_footprint_histogram']}
    result['analysis_ms_per_process']=[row['analysis_total_ms'] for row in rows]
    result['validation_ms_per_process']=[row['validation_ms'] for row in rows]
    peak=[]
    for repeat in range(2):
        lines=(root/f'{case}-r{repeat}.time').read_text().splitlines()
        assert any(line.strip()=='Exit status: 0' for line in lines)
        peak.append(int(next(line.split(':',1)[1].strip() for line in lines if 'Maximum resident set size (kbytes)' in line))*1024)
    result['maximum_rss_bytes_per_process']=peak
    summary[case]=result

old,new=summary['city-original'],summary['city-current']
for key in ['records','logical_payload_sha256','insertion_order_reuse','first_seen_4mib_cache_model']:
    assert old[key]==new[key],key
for key in ['distinct_values','logical_bytes','unique_bytes','frequency_histogram','family_unique_value_masks','family_references_and_logical_bytes']:
    assert old['payloads'][key]==new['payloads'][key],key
assert old['first_seen_4mib_cache_model']['miss_payload_bytes']==old['physical_layout']['pool_bytes']
assert old['first_seen_4mib_cache_model']['misses']==old['payloads']['physical_nonempty_spans']
assert new['payloads']['duplicate_stored_bytes']==0
asn=json.loads((root/'asn-source.json').read_text())
assert asn['logical_payload_sha256']==summary['asn']['logical_payload_sha256']
ingest=json.loads((root/'ingest.log').read_text())
assert asn['records']==summary['asn']['records']==ingest['full_original_assertions_checked']==ingest['records']
output={'revision':receipt['revision'],'receipt_sha256':sha(root/'receipt.json'),
    'files_verified':len(receipt['files']),'processes':6,'cpu':receipt['cpu'],'platform':receipt['platform'],
    'asn_source':asn,'datasets':summary}
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps({key:value for key,value in output.items() if key not in ['datasets','asn_source']}))
