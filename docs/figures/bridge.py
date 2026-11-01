"""Small Jupyter-to-R bridge. Mathematical/reference calculations stay in notebooks."""
import json,os,subprocess,re,html
from pathlib import Path
HERE=Path(__file__).resolve().parent
STEMS={'approximation':['logistic-mixed-integration-error','logistic-mixed-node-sensitivity'],
       'public-agreement':['lmm-public-agreement'],
       'sleepstudy':['sleepstudy-slopes'],'profile':['lmm-profile'],'posterior':['logistic-mixed-posterior']}
def render_figure(job,payload=None,display=True):
    if payload is not None:
        (HERE/'data'/f'{STEMS[job][0]}.json').write_text(json.dumps(payload,indent=2,allow_nan=False)+'\n')
    subprocess.run([os.environ.get('RSCRIPT','Rscript'),str(HERE/'render.R'),job],check=True,cwd=HERE.parent.parent)
    if display:
        from IPython.display import SVG,display as show
        for stem in STEMS[job]:
            text=(HERE/f'{stem}.svg').read_text()
            # Inline SVGs share a document; namespace clip IDs across figures.
            for ident in set(re.findall(r'id="([^"]+)"',text)):
                replacement=f'{stem}-{ident}'
                text=text.replace(f'id="{ident}"',f'id="{replacement}"').replace(f'url(#{ident})',f'url(#{replacement})')
            titles={'lmm-profile':'Gaussian likelihood profile on the teaching sample',
                    'logistic-mixed-posterior':'Conditional random-intercept distribution and Gaussian approximation',
                    'logistic-mixed-integration-error':'Independent integration error across quadrature node counts',
                    'logistic-mixed-node-sensitivity':'Group standard deviation sensitivity to quadrature node count',
                    'lmm-public-agreement':'PHP and lme4 agreement on published Gaussian datasets',
                    'sleepstudy-slopes':'Observed reaction times, population mean and subject conditional means'}
            title=titles[stem]
            title_id=f'{stem}-accessible-title'
            text=re.sub(r'(<svg\b[^>]*>)',lambda m:m.group(1).replace('<svg ',f'<svg role="img" aria-labelledby="{title_id}" ',1)+f'<title id="{title_id}">{html.escape(title)}</title>',text,count=1)
            show(SVG(data=text))
