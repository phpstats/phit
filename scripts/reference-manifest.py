#!/usr/bin/env python3
"""Index saved reference inputs/options, versions, quantities and test policies; no R needed."""
import hashlib,json,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
GENERATORS={'ols-r':'tests/generate-fixtures.R','logistic-r':'tests/generate-logistic-fixtures.R',
 'probit-r':'tests/generate-probit-poisson-fixtures.R','poisson-r':'tests/generate-probit-poisson-fixtures.R',
 'r-compatibility':'tests/generate-r-compatibility-fixtures.R','lmm-r':'tests/generate-lmm-fixtures.R',
 'lmm-public-r':'tests/generate-lmm-public-fixtures.py','lmm-expanded-r':'tests/generate-lmm-expanded-fixtures.R',
 'logistic-mixed-r':'tests/generate-logistic-mixed-fixtures.R','stress-r':'tests/generate-stress-fixtures.R',
 'draws-python':'tests/generate-draw-fixtures.py',
 'distributions-r':'tests/generate-distribution-fixtures.R',
 'distributions-scipy':'docs/verifications/compare_distributions.py',
 'distributions-mpmath':'docs/verifications/compare_distributions.py',
 'descriptive-r':'tests/generate-distribution-fixtures.R',
 'rng-python-r':'tests/generate-rng-fixtures.py',
 'formula-r':'tests/generate-formula-fixtures.R',
 'formula-lme4-r':'tests/generate-formula-lme4-fixtures.R',
 'predict-r':'tests/generate-predict-fixtures.R',
 'smoothing-r':'tests/generate-smoothing-fixtures.R',
 'optimize-r':'tests/generate-optimize-fixtures.R'}
POLICY={'ols-r':'1e-9 + 1e-7*abs(reference); zero/perfect fit conventions tested separately',
 'logistic-r':'1e-9 + 1e-7*abs(reference)','probit-r':'1e-9 + 1e-7*abs(reference)','poisson-r':'1e-9 + 1e-7*abs(reference)',
 'r-compatibility':'1e-7 + 1e-6*abs(reference)',
 'lmm-r':'1e-7 + 1e-6*abs(reference)','lmm-public-r':'per-case/per-quantity policy stored in fixture',
 'lmm-expanded-r':'likelihood: 5e-6 + 1e-9*abs(reference); estimates versus R bobyqa: 1e-5 + 1e-5*abs(reference); other optimizers: 1e-3 + 3e-4*abs(reference)',
 'logistic-mixed-r':'inference: 2e-5 + 2e-3*abs(reference); likelihood/AIC/BIC: 1e-6 + 1e-8*abs(reference); others: 2e-5 + 2e-4*abs(reference)',
 'stress-r':'per-case absolute+relative; absolute scales with response_scale for estimates and its square for covariance; dimensionless quantities unscaled',
 'draws-python':'1e-11 + 1e-11*abs(reference); extreme scale nonzero outputs/identities also relative 1e-12; inverse round trips 1e-11 + 1e-11*abs(reference)',
 'distributions-r':'Densities/CDF/SF: 1e-300 + 1e-12*abs(reference); logs: 1e-12*max(1,abs(reference)); quantiles: 1e-15 + 1e-12*abs(reference). Immutable mpmath references adjudicate R disagreements; documented boundary differences tested separately.',
 'distributions-scipy':'Independent peer values and error report; not an authoritative release-gate replacement for R/mpmath.',
 'distributions-mpmath':'Same operation-specific policy as R; 60-digit references rounded once, null denotes unavailable. Reference choice uses only saved R/mpmath values, never the native result.',
 'descriptive-r':'Relative 1e-12, exact zero; scalar/batch type-7 quantiles must agree exactly. Missing/error conventions tested separately.',
 'formula-r':'model.matrix columns/terms/rows exact; values 1e-12 + 1e-12*abs(reference); lm/glm fits 1e-9 + 1e-7*abs(reference) (orthogonal poly() designs use the fit tolerance). GLM references are polished by restarting glm() from its own solution.',
 'formula-lme4-r':'lmer as lmm-expanded-r bobyqa (likelihood/AIC/BIC 5e-6 + 1e-9*abs(reference); estimates 1e-5 + 1e-5*abs(reference)); glmer as logistic-mixed-r (likelihood 1e-6 + 1e-8*abs(reference); SE/covariance 2e-5 + 2e-3*abs(reference); others 2e-5 + 2e-4*abs(reference))',
 'predict-r':'fit/intervals/se 1e-7 + 1e-6*abs(reference); GLM se_fit_final (information at the final coefficients, recomputed in R) the same, R predict.glm se.fit 1e-6 + 1e-3*abs(reference); lmer 1e-5 + 1e-5*abs(reference); glmer 2e-5 + 2e-4*abs(reference); missing rows exact. GLM references use glm.control(epsilon = 1e-14).',
 'smoothing-r':'Relative 1e-12 for bandwidths, grids and interpolated values; density heights 1e-12 times the peak height; ECDF proportions and n exact. NA (null) must be NAN. Missing/error conventions tested separately.',
 'optimize-r':'Exact: optimum, objective, root, f(root), iterations, estimated precision and the number of function evaluations must equal R bit for bit; convergence warnings must agree.',
 'rng-python-r':'Independent ChaCha/seed/Open01 uniforms exact; R inverse-CDF values 1e-15 + 1e-12*abs(reference); discrete/point-mass samples exact. Prefix/repeatability, limits and moments tested separately.'}
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def compact_ref(ref):return {k:v for k,v in ref.items() if k not in ['result','results','expected']}
report={'schema_version':1,'purpose':'Reference provenance index; test sources and fixtures are authoritative, not a new snapshot of estimates.','fixtures':[]}
for stem,generator in GENERATORS.items():
 path=ROOT/'tests/fixtures'/f'{stem}.json';data=json.loads(path.read_text());cases=[]
 source_cases=data.get('cases',[])
 if stem=='descriptive-r':
  source_cases=[{**c,'section':section,'expected':{k:v for k,v in c.items() if k not in ['name','values','x','y','weights','quantile_probabilities']}}
    for section in ['descriptive','pairs','weighted'] for c in data[section]]
 if stem=='optimize-r':
  source_cases=[{'section':section,'name':c['f'],'inputs':{k:v for k,v in c.items() if k!='expected'},'expected':c['expected']}
    for section in ['optimize','uniroot'] for c in data[section]]
 if stem=='smoothing-r':
  source_cases=[{'section':section,'name':c['name'],'inputs':{k:v for k,v in c.items() if k not in ['name','expected','nrd0','nrd']},
    'expected':c['expected'] if 'expected' in c else {'nrd0':c['nrd0'],'nrd':c['nrd']}}
    for section in ['density','bandwidth','approx','ecdf'] for c in data[section]]
 for c in source_cases:
  inputs={k:c[k] for k in ['model','x','y','groups','group_labels','terms','weights','offset','reml','quadrature_points',
    'operation','draws','normalization','source_covariance','target_covariance','matrix','shift','inverse','inverse_shift',
    'distribution','params','input','section','values','quantile_probabilities','seed','n'] if k in c}
  expected=c.get('expected',c.get('reference',{}).get('result',{}))
  references=c.get('references',{})
  if references:expected=next(iter(references.values()))['result']
  metadata={k:c[k] for k in ['name','dataset','source','tolerance','response_scale','distribution','operation','section'] if k in c}
  if isinstance(c.get('reference'),dict):metadata['reference']=compact_ref(c['reference'])
  if references:metadata['references']={k:compact_ref(v) for k,v in references.items()}
  if 'nlme_reference' in c:metadata['nlme_reference']=compact_ref(c['nlme_reference'])
  if stem=='optimize-r':
   inputs={**c['inputs'],'section':c['section']};n=1;p=1;quantities=list(expected)
  elif stem=='smoothing-r':
   inputs={**c['inputs'],'section':c['section']};n=len(c['inputs']['x']);p=1
   quantities=list(expected) if isinstance(expected,dict) else [c['section']]
  elif stem=='predict-r':
   inputs={k:c[k] for k in ['function','formula','args','options'] if k in c}
   inputs['data']=data['datasets'][c['dataset']];inputs['newdata']=data['newdata'][c['dataset']]
   n=len(expected['fit']);p=0;quantities=list(expected)
  elif stem in ['formula-r','formula-lme4-r']:
   inputs={k:c[k] for k in ['function','formula','family','link','weights','reml','quadrature_points','bars'] if k in c}
   inputs['data']=data['datasets'][c['dataset']]
   n=expected['n_observations'] if 'n_observations' in expected else len(expected['retained_rows']);p=len(expected['columns']);quantities=list(expected)
  elif stem.startswith('distributions-') or stem=='rng-python-r':
   n=len(c['input']);p=1;quantities=[c['operation']]
  elif stem=='descriptive-r':
   n=len(c.get('values',c.get('x',[])));p=1;quantities=list(expected)
  else:
   n=len(c.get('y',c.get('draws',c.get('source_covariance',[]))))
   p=len(c.get('x',c.get('draws',c.get('source_covariance',[[]])))[0]);quantities=list(expected)
  cases.append({**metadata,'inputs_sha256':hashlib.sha256(json.dumps(inputs,sort_keys=True,separators=(',',':')).encode()).hexdigest(),
    'n':n,'p':p,'compared_quantities':quantities})
 report['fixtures'].append({'path':str(path.relative_to(ROOT)),'sha256':sha(path),'generator':generator,'generator_sha256':sha(ROOT/generator),
  'software_recorded_in_fixture':{k:v for k,v in data.items() if k not in ['cases','descriptive','pairs','weighted','datasets','newdata','density','bandwidth','approx','ecdf','optimize','uniroot']},'tolerance_policy':POLICY[stem],
  'model_specification':('Literal draw/covariance matrices and options; 80-digit independent Decimal formulas and decompositions.' if stem=='draws-python' else
    'Explicit saved matrices/vectors/options in fixture; generator records reference formula, family, seed and controls.'),
  'cases':cases})
 if stem.startswith('distributions-') or stem in ['descriptive-r','smoothing-r']:
  report['fixtures'][-1]['model_specification']='Explicit distribution parameterization/evaluation points or numeric descriptive slices; no model fit or data filtering in reference generation.'
 if stem=='formula-lme4-r':
  report['fixtures'][-1]['model_specification']='Named-column datasets (schools, cbpp, seeded crossed/nested panel) with lme4 formula strings; lmer/glmer bobyqa rhoend 1e-9 (glmer tolPwrss 1e-12) under LC_COLLATE=C.'
 if stem=='predict-r':
  report['fixtures'][-1]['model_specification']='Named-column datasets (mtcars, warpbreaks, cbpp, schools, seeded panel) and newdata frames with unseen values, missing rows and new groups; R lm/glm/lmer/glmer then predict() under LC_COLLATE=C.'
 if stem=='formula-r':
  report['fixtures'][-1]['model_specification']='Named-column datasets (repository CSVs and a seeded synthetic frame) with formula strings; R model.frame/model.matrix/lm/glm under LC_COLLATE=C.'
 if stem=='rng-python-r':
  extra=ROOT/'tests/generate-rng-fixtures.R'
  report['fixtures'][-1]['reference_inputs']=[{'path':str(extra.relative_to(ROOT)),'sha256':sha(extra)}]
  report['fixtures'][-1]['model_specification']='Independent Python ChaCha20/PCG/Open01 uniforms and explicit seeds; base-R quantiles transform those exact uniform values. No native RNG values are used as references.'
 if stem in ['distributions-scipy','distributions-mpmath']:
  dependency=ROOT/'tests/fixtures/distributions-r.json'
  report['fixtures'][-1]['reference_inputs']=[{'path':str(dependency.relative_to(ROOT)),'sha256':sha(dependency)}]
  original=json.loads(dependency.read_text())['cases']
  if len(original)!=len(source_cases) or any({k:c[k] for k in ['distribution','params','operation','input']} !=
      {k:r[k] for k in ['distribution','params','operation','input']} for c,r in zip(source_cases,original)):
   raise SystemExit(f'{stem}: independent fixture inputs do not align with R')
output=ROOT/'docs/verifications/reference-manifest.json';text=json.dumps(report,indent=2)+'\n'
if '--check' in sys.argv:
 if output.read_text()!=text:raise SystemExit('Reference manifest drifted; regenerate scripts/reference-manifest.py and review evidence changes')
 print('Reference provenance hashes verified')
else:output.write_text(text);print('Reference provenance manifest generated')
