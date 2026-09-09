"""Run from the repository root: python3 docs/work/file-validation-0.6.0/verify_archives.py PACKAGE_DIRECTORY OUTPUT_JSON EXPECTED_COMMIT.
Checks clean VCS identity, versions, dependencies, notices, Rust source bytes and complete gzip consumption.
"""
from pathlib import Path
import datetime,hashlib,json,subprocess,sys,tarfile,tomllib,zlib
root=Path(__file__).resolve().parents[3]; source=Path(sys.argv[1]); output=Path(sys.argv[2]); expected=sys.argv[3]
report={'checked_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_commit':expected,'directory':str(source),'packages':[]}
for name in ['astro-io','astro-metadata','astro-metrics','ravensky-astro']:
 path=source/f'{name}-0.6.0.crate'; payload=path.read_bytes(); dec=zlib.decompressobj(31); dec.decompress(payload)
 assert dec.eof and not dec.unused_data, f'trailing or incomplete gzip: {name}'
 with tarfile.open(path) as archive:
  names=archive.getnames(); prefix=f'{name}-0.6.0/'
  manifest=tomllib.loads(archive.extractfile(prefix+'Cargo.toml').read().decode())
  assert manifest['package']['version']=='0.6.0' and manifest['package']['rust-version']=='1.94'
  assert 'workspace' not in manifest and 'patch' not in manifest
  for section in ['dependencies','build-dependencies']:
   for dep,spec in manifest.get(section,{}).items():
    assert 'path' not in spec
    if dep in ['astro-io','astro-metadata','astro-metrics']:assert spec['version']=='0.6.0'
  vcs=json.load(archive.extractfile(prefix+'.cargo_vcs_info.json'))
  assert vcs['git']['sha1']==expected and not vcs['git'].get('dirty',False)
  base=root if name=='ravensky-astro' else root/name
  rust_files=0
  for member in archive.getmembers():
   rel=member.name.removeprefix(prefix)
   if member.isfile() and rel.endswith('.rs'):
    assert archive.extractfile(member).read()==(base/rel).read_bytes(),rel
    rust_files+=1
  if name=='astro-io':
   for notice in ['CFITSIO_LICENSE','HCOMPRESS_LICENSE','fitskit-MIT.txt']:
    assert archive.extractfile(prefix+'licenses/'+notice).read()==(base/'licenses'/notice).read_bytes()
  assert not any('/docs/benchmarks/' in n for n in names)
  report['packages'].append({'name':name,'version':'0.6.0','sha256':hashlib.sha256(payload).hexdigest(),'archive_bytes':len(payload),'files':len(names),'rust_files_matched':rust_files,'internal_dependencies':{n:s['version'] for n,s in manifest.get('dependencies',{}).items() if n in ['astro-io','astro-metadata','astro-metrics']},'clean_vcs':True,'complete_gzip':True})
output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
