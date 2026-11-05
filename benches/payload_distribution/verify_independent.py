"""Independent Python byte/slot counts, reference gaps and payload-byte LRU.

No Rust analyzer data structures or parser code are reused. Provider bytes remain
in memory/private files; only aggregates are emitted. LRU equivalence is checked
only after requiring every input value to fit the 4 MiB payload-only budget.
"""
import argparse
import collections
import hashlib
import json
import mmap
import pathlib
import struct

p=argparse.ArgumentParser()
p.add_argument('directory',type=pathlib.Path)
p.add_argument('--output',type=pathlib.Path,required=True)
args=p.parse_args()
results={}
for case in ['city-original','city-current','asn']:
    expected=json.loads((args.directory/f'{case}-r0.log').read_text())
    with (args.directory/'inputs'/f'{case}.nidx').open('rb') as source, mmap.mmap(source.fileno(),0,access=mmap.ACCESS_READ) as data:
        magic,version,flags,n,v4,v6,asn,meta,pool,p4,p6=struct.unpack_from('<8sII8Q',data)
        assert magic==b'NETINDEX' and version==3 and n==v4+v6+asn
        slots=80+meta+(p4 if flags&1 else v4*20)+(p6 if flags&2 else v6*56)+asn*8
        if flags&8:
            length,width=struct.unpack_from('<IB',data,slots)
            base=slots+8+n*width
            def span(at):return int.from_bytes(data[slots+8+at*width:slots+8+(at+1)*width],'little')*length,length
        else:
            fmt='<II' if flags&4 else '<QQ'
            width=struct.calcsize(fmt)
            base=slots+n*width
            def span(at):return struct.unpack_from(fmt,data,slots+at*width)
        assert base+pool==len(data)
        values={}
        lru=collections.OrderedDict()
        resident=lru_hits=logical=unique=written=physical=0
        lengths=collections.Counter()
        gaps=collections.Counter()
        digest=hashlib.sha256(b'netindex-payload-stream-v1\0')
        for at in range(n):
            offset,length=span(at)
            assert offset+length<=pool and length<=4*1024*1024
            payload=data[base+offset:base+offset+length]
            digest.update(struct.pack('<Q',length))
            digest.update(payload)
            lengths[length]+=1
            logical+=length
            if length and offset==written:
                written+=length
                physical+=1
            if (state:=values.get(payload)) is None:
                values[payload]=[at,1]
                unique+=length
            else:
                gaps[(at-state[0]).bit_length()]+=1
                state[0]=at
                state[1]+=1
            if payload in lru:
                lru_hits+=1
                lru.move_to_end(payload)
            else:
                lru[payload]=length
                resident+=length
                while resident>4*1024*1024:
                    _,size=lru.popitem(last=False)
                    resident-=size
        histogram=[{'low':1<<(bucket-1),'high':(1<<bucket)-1,'count':count} for bucket,count in sorted(gaps.items())]
        frequencies=dict(collections.Counter(state[1] for state in values.values()))
        assert written==pool
        assert len(values)==expected['payloads']['distinct_values']
        assert unique==expected['payloads']['unique_bytes']
        assert logical==expected['payloads']['logical_bytes']
        assert physical==expected['payloads']['physical_nonempty_spans']
        assert histogram==expected['insertion_order_reuse']['reference_gap_histogram']
        assert lru_hits==expected['insertion_order_reuse']['within_byte_budget']['4194304']
        assert {str(key):value for key,value in frequencies.items()}==expected['payloads']['frequency_histogram']
        assert dict(lengths)=={row['length']:row['references'] for row in expected['payloads']['lengths']}
        assert digest.hexdigest()==expected['logical_payload_sha256']
        results[case]={'records':n,'distinct_values':len(values),'unique_bytes':unique,
            'logical_bytes':logical,'physical_nonempty_spans':physical,
            'reference_gaps_match':True,'frequency_and_length_histograms_match':True,
            'payload_only_4mib_lru_hits':lru_hits,'logical_payload_sha256':digest.hexdigest()}
output={'script_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
    'receipt_sha256':hashlib.sha256((args.directory/'receipt.json').read_bytes()).hexdigest(),'datasets':results}
args.output.write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps(output))
