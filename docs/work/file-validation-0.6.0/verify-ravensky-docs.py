"""Check versioned hosted API pages: python3 verify-ravensky-docs.py REPOSITORY."""
from pathlib import Path
import datetime, json, sys, urllib.request, urllib.error

reports = []
for name, module in [('astro-io','astro_io/validation/'), ('astro-metadata','astro_metadata/'), ('astro-metrics','astro_metrics/'), ('ravensky-astro','ravensky_astro/')]:
    url = f'https://docs.rs/{name}/0.6.0/{module}'
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            body = response.read().decode()
            ok = response.status == 200 and '0.6.0' in body and 'rustdoc' in body and response.url == url
            reports.append({'url': url, 'final_url': response.url, 'http_status': response.status, 'versioned_rustdoc_verified': ok})
    except urllib.error.HTTPError as error:
        reports.append({'url':url, 'http_status':error.code, 'versioned_rustdoc_verified':False})
result = {'checked_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'pages':reports}
(Path(sys.argv[1]) / 'target/docs-rs-060.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
raise SystemExit(0 if all(r['versioned_rustdoc_verified'] for r in reports) else 1)
