#!/usr/bin/env python3
"""Reconcile executed round-2 evidence and immutable source/grant bytes; no staging."""
import hashlib,json,os,pathlib,re,subprocess
V2=pathlib.Path(__file__).resolve().parents[1];ROOT=V2.parent
assert pathlib.Path(os.environ['HOME']).is_relative_to(V2/'.qa')
sha=lambda b:hashlib.sha256(b).hexdigest()
load=lambda p:json.loads((V2/p).read_text())
source=load('fixtures/pinned-sources.json');repos={'source':ROOT,'sdk':ROOT.parent/'tezbar-extension-sdk-rust','host':ROOT.parents[1]/'Tezbar'}
for row in source:
 b=subprocess.check_output(['git','-C',str(repos[row['kind']]),'show',row['commit']+':'+row['path']]);assert sha(b)==row['sha256']
 assert (V2/'.qa/pinned'/row['kind']/row['path']).read_bytes()==b
# Preserve the original format fixtures byte-for-byte; append new fixtures separately.
for row in load('fixtures/generated-media.json'):
 assert sha((V2/'.qa/fixtures'/row['path']).read_bytes())==row['sha256']
for row in load('fixtures/round2-generated-media.json'):
 assert sha((V2/'.qa/round2-fixtures'/row['path']).read_bytes())==row['sha256']
old=load('REVIEW-INVENTORY.json');immutable=[r for r in old['files']if r['path'].startswith(('v2/licenses/','v2/fixtures/','v2/resources/'))]
for row in immutable:assert sha((ROOT/row['path']).read_bytes())==row['sha256'],'immutable candidate fixture/grant changed: '+row['path']
# After the first round-2 inventory, this also verifies any newly captured fixture bytes.
assert subprocess.check_output(['git','-C',str(ROOT),'diff','--cached','--name-only'])==b''
gates=load('evidence/rust-gates.json');process=load('evidence/process-qa.json');extracted=load('evidence/extracted-process-qa.json');archive=load('evidence/archive.json');fresh=load('evidence/fresh-build.json')
assert gates['requiredFailures']==0 and gates['passed']==8
assert process['caseCount']==extracted['caseCount']==13
assert all(c['state']=='passed'for c in process['cases']+extracted['cases'])
assert archive['sourceSha256']==fresh['productionSourceSha256']
h=hashlib.sha256()
for p in sorted([V2/'Cargo.toml',V2/'Cargo.lock',*V2.joinpath('src').rglob('*.rs')]):h.update(str(p.relative_to(V2)).encode()+b'\0'+hashlib.sha256(p.read_bytes()).digest())
assert h.hexdigest()==archive['sourceSha256']
count=lambda label:sum(map(int,re.findall(r'test result: ok\. (\d+) passed', (V2/('evidence/'+label+'-test.txt')).read_text())))
report={'root':str(ROOT),'branch':subprocess.check_output(['git','-C',str(ROOT),'branch','--show-current'],text=True).strip(),'sourceHead':subprocess.check_output(['git','-C',str(ROOT),'rev-parse','HEAD'],text=True).strip(),'rustGatesPassed':gates['passed'],'rustGatesBlocked':gates['blocked'],'rustTestCounts':{'stable':count('stable'),'msrv':count('msrv')},'v1SourceOracleCases':load('evidence/v1-oracles.json')['caseCount'],'sdkProcessScenarios':process['caseCount'],'extractedProductionScenarios':extracted['caseCount'],'originalContainerVariants':12,'additionalFormatFixtures':8,'nativeSuccessfulTransformations':15,'decodedTransformationParityVariants':14,'metadataOnlyXmpTransformations':1,'originalNativeToolReadOnlyErrors':3,'additionalNativeErrors':['BMP read-only','fragmented MP4 native write error'],'schemaNegativeCases':load('evidence/schema-qa.json')['negativeCount'],'cargoDependencyPackages':108,'authenticDependencyGrantFiles':198,'missingGrantTexts':[],'iconSha256':sha((V2/'resources/extension-icon.png').read_bytes()),'archiveSha256':archive['archiveSha256'],'archiveClosedMembers':archive['closedMembers'],'productionSourceSha256':archive['sourceSha256'],'freshBinarySha256':fresh['binarySha256'],'immutablePinnedGitObjectsVerified':len(source),'immutablePriorFixtureGrantResourceFilesVerified':len(immutable),'originalGeneratedFixtureBytes':'unchanged','indexEmpty':True,'xerjFinalIndex':'incomplete; frozen schema; no bypass/restart','desktopVerification':{'macOS':'not-run','Windows':'not-run'},'remaining':['Native ExifTool is trusted for layouts outside finite encoded-payload witnesses; RAW/BigTIFF/JXL/further document/archive/codec variants not independently exercised','Unapplied generic bounded/complete directory-preserving selection host prerequisite','Real macOS GUI acceptance','Native Windows/MSVC/linker/filesystem/GUI QA; GNU dlltool absent','PDF metadata can remain recoverable; protected macOS xattrs can remain; warnings/limitations reported partial','POSIX final pathname fence is optimistic, not same-user atomic CAS; crash exactly-once durability not claimed'],'scope':'Only own clone v2/ plus exact short root README link; no stage/commit/push/PR/install/release/deletion/subdelegation/host-or-SDK edits'}
assert report['branch']=='feat/native-v2-rust'and report['sourceHead']=='0700b381975cccdfdadc2cd91c83171782875fa7'
(V2/'evidence/final-report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
