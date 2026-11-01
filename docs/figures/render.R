#!/usr/bin/env Rscript
# All figures use saved results or notebook-exported points; never fit/time a model.
args <- commandArgs(trailingOnly=TRUE)
file_arg <- sub('^--file=','',grep('^--file=',commandArgs(),value=TRUE)[1])
HERE <- dirname(normalizePath(file_arg)); DOCS <- dirname(HERE)
for(package in c('ggplot2','jsonlite','svglite')) if(!requireNamespace(package,quietly=TRUE)) stop('Install documentation R package: ',package)
source(file.path(HERE,'style.R'))
theme_set(figure_theme())
read_json <- function(path) jsonlite::fromJSON(path,simplifyVector=FALSE)
v <- function(x) unlist(x,use.names=FALSE)
bind <- function(x) do.call(rbind,x)
manifest_path <- file.path(HERE,'manifest.json')
manifest <- if(file.exists(manifest_path)) read_json(manifest_path) else list(figures=list())
md5 <- function(path) unname(tools::md5sum(path))
# Manifest paths are portable and relative to docs/, including root fixtures.
relative <- function(path) {
 path <- normalizePath(path)
 docs_prefix <- paste0(DOCS,'/')
 repo_prefix <- paste0(dirname(DOCS),'/')
 if(startsWith(path,docs_prefix)) return(substring(path,nchar(docs_prefix)+1))
 if(startsWith(path,repo_prefix)) return(paste0('../',substring(path,nchar(repo_prefix)+1)))
 stop('Figure input/output is outside the repository: ',path)
}
current_inputs <- character()
save_figure <- function(plot,stem,width=10,height=6) {
 stopifnot(inherits(plot,'ggplot'));dir.create(dirname(stem),recursive=TRUE,showWarnings=FALSE)
 # Explicit devices/dimensions; SVG text stays selectable and PNG is portable.
 ggsave(paste0(stem,'.svg'),plot=plot,device=svglite::svglite,width=width,height=height,units='in',bg='white')
 ggsave(paste0(stem,'.png'),plot=plot,device=grDevices::png,type='cairo',width=width,height=height,units='in',dpi=160,bg='white')
 key <- relative(paste0(stem,'.svg'))
 manifest$figures[[key]] <<- list(width_inches=width,height_inches=height,png_dpi=160,
  inputs=lapply(current_inputs,function(path)list(path=relative(path),md5=md5(path))),
  svg_md5=md5(paste0(stem,'.svg')),png_md5=md5(paste0(stem,'.png')))
 cat(key,'\n')
}
benchmark <- function(model) {
 path <- file.path(DOCS,'benchmarks',paste0(model,'-results.json'));current_inputs <<- path;r <- read_json(path)
 cases <- r$cases[order(vapply(r$cases,function(c)c$n,0),vapply(r$cases,function(c)c$p,0))]
 order <- if(model=='logistic-mixed') c('R','SciPy reference','PHP') else c('R','statsmodels','PHP')
 rows <- samples <- list()
 for(i in seq_along(cases)){
  case<-cases[[i]];panel<-sprintf('%s observations | %s coefficients',number_label(case$n),case$p)
  for(j in seq_along(order)){
   fit<-case$implementations[[order[j]]];if(is.null(fit))stop('Missing implementation: ',order[j])
   memory<-if(is.null(fit$peak_rss_bytes)) NA_real_ else fit$peak_rss_bytes/2^20
   rows[[length(rows)+1]]<-data.frame(panel=panel,implementation=order[j],x=j,time=fit$median_fit_seconds*1000,memory=memory)
   times<-v(fit$fit_seconds)*1000
   samples[[length(samples)+1]]<-data.frame(panel=panel,implementation=order[j],x=j+seq(-.14,.14,length.out=length(times)),value=times)
  }
 }
 d<-bind(rows);points<-bind(samples);levels<-unique(d$panel)
 d$panel<-factor(d$panel,levels=levels);points$panel<-factor(points$panel,levels=levels)
 title<-switch(model,ols='OLS',logistic='Logistic',probit='Probit',poisson='Poisson',lmm='Gaussian random intercept',`logistic-mixed`='Binomial random intercept: Laplace ML')
 for(metric in c('time','memory')){
  d$value<-d[[metric]];d$label<-measurement_label(d$value);d$label_y<-ifelse(is.na(d$value),0,d$value)
  p<-ggplot(d,aes(x=x,y=value,fill=implementation))+geom_col(width=.58,alpha=.85,na.rm=TRUE)+
   geom_text(aes(y=label_y,label=label),vjust=-.7,size=3.5,show.legend=FALSE)+
   scale_fill_manual(values=implementation_colors,limits=order)+scale_x_continuous(breaks=seq_along(order),labels=order)+
   scale_y_continuous(expand=expansion(mult=c(0,.22)))+facet_wrap(~panel,ncol=2,scales='free_y',axes='all_x')+
   labs(title=paste(title,if(metric=='time')'| Full-fit time' else '| Peak process memory'),
    subtitle=if(metric=='time')paste('Median of',r$repeats,'fits; dots show individual repetitions') else 'Whole worker process, including runtime, inputs and results',
    x=NULL,y=if(metric=='time')'Full fit (ms)' else 'Peak process RSS (MiB)',
    caption=if(metric=='time')'Each panel uses its own linear scale. Startup/CSV loading excluded. 0* can reflect timer resolution.' else 'Each panel uses its own linear scale. Missing memory is not shown as a measured zero.')+
   theme(legend.position='none')
  if(metric=='time')p<-p+geom_point(data=points,aes(x=x,y=value,shape=implementation),inherit.aes=FALSE,color='#253247',size=1.6,alpha=.75)+scale_shape_manual(values=implementation_shapes,limits=order)
  save_figure(p,file.path(DOCS,'benchmarks','figures',paste0(model,if(metric=='time')'-fit-time' else '-memory')),10,if(length(cases)>2)7.3 else 5)
 }
}
approximation <- function(){
 path<-file.path(DOCS,'verifications','logistic-mixed-results.json');current_inputs<<-path;r<-read_json(path)
 d<-bind(lapply(r$results,function(x)data.frame(dataset=x$dataset,nodes=x$quadrature_points,error=abs(x$approximation_log_likelihood_error),sd=x$PHP$random_intercept_sd)))
 d<-d[d$dataset!='boundary',];d$display_error<-pmax(d$error,1e-13)
 labels<-c(binary='Binary',no_intercept='No intercept',grouped_offset='Grouped trials + offset',rare_events='Rare events',large_group_variation='Large group variation',cbpp='cbpp')
 d$dataset<-factor(d$dataset,levels=names(labels));colors<-setNames(c('#0072B2','#D55E00','#009E73','#CC79A7','#E69F00','#333333'),names(labels))
 p<-ggplot(d,aes(nodes,display_error,color=dataset,shape=dataset))+geom_line()+geom_point(size=2.5)+
  scale_color_manual(values=colors,labels=labels)+scale_shape_manual(values=seq(15,20),labels=labels)+
  scale_y_log10()+scale_x_continuous(breaks=c(1,9,25,61))+
  labs(title='Integration accuracy is separate from implementation agreement',x='Quadrature nodes (1 = Laplace)',y='Absolute log-likelihood approximation error',
   subtitle='Independent integration evaluated at each approximation\'s own fitted parameters',caption='Display floor: 1e-13 for near-zero errors; saved reports retain unmodified values. Logarithmic vertical scale.')
 save_figure(p,file.path(HERE,'logistic-mixed-integration-error'),10,5.5)
 rare<-d[d$dataset=='rare_events',]
 p<-ggplot(rare,aes(nodes,sd))+geom_line(color=unname(concept_colors['conditional']))+geom_point(color=unname(concept_colors['conditional']),size=2.5)+
  scale_x_continuous(breaks=c(1,9,25,61))+labs(title='Rare events: the approximation changes the estimate',x='Quadrature nodes (1 = Laplace)',y='Estimated random-intercept SD (log-odds units)',caption='Same data and model at every node count; this is approximation sensitivity, not a comparison of languages.')
 save_figure(p,file.path(HERE,'logistic-mixed-node-sensitivity'),9,4.6)
}
public_agreement <- function(){
 path<-file.path(DOCS,'verifications','lmm-public-results.json');current_inputs<<-path;r<-read_json(path)
 d<-bind(lapply(r,function(c){ref<-v(c$R$fitted);data.frame(dataset=c$name,reference=ref,difference=1e6*(v(c[['phit']]$fitted)-ref))}))
 d$dataset<-factor(d$dataset,levels=vapply(r,function(c)c$name,''))
 p<-ggplot(d,aes(reference,difference))+geom_hline(yintercept=0,color='#64748B',linetype='dashed')+
  geom_point(color=unname(concept_colors['conditional']),alpha=.35,size=1.2)+facet_wrap(~dataset,ncol=2,scales='free')+
  labs(title='Gaussian mixed models: agreement on every observation',x='lme4 conditional fitted value (dataset outcome units)',y='PHP - lme4 (millionths of outcome unit)',
   caption='Zero denotes numerical agreement. Panel units/scales differ; this is not a residual or model-adequacy diagnostic.')
 save_figure(p,file.path(HERE,'lmm-public-agreement'),10,6.5)
}
sleepstudy <- function(){
 path<-file.path(DOCS,'verifications','r-compatibility-results.json');fixture<-file.path(dirname(DOCS),'tests','fixtures','lmm-expanded-r.json');csv<-file.path(DOCS,'data','lmm','sleepstudy.csv')
 current_inputs<<-c(path,fixture,csv);r<-read_json(path);f<-read_json(fixture);data<-read.csv(csv)
 case<-Filter(function(c)c$name=='sleepstudy_reml',r$mixed)[[1]];fit<-case$PHP$bobyqa
 input<-Filter(function(c)c$name=='sleepstudy_reml',f$cases)[[1]];groups<-v(input$terms[[1]]$groups);stopifnot(length(groups)==nrow(data))
 stopifnot(isTRUE(all.equal(data$Days,v(lapply(input$x,function(row)row[[2]])),tolerance=0)),isTRUE(all.equal(data$Reaction,v(input$y),tolerance=0)))
 beta<-v(fit$coefficients);labels<-v(fit$random[[1]]$group_labels);subjects<-head(sort(unique(data$Subject)),6)
 lines<-observed<-list()
 for(subject in subjects){
  rows<-data[data$Subject==subject,];group<-groups[which(data$Subject==subject)[1]];index<-match(group,labels);stopifnot(!is.na(index));b<-v(fit$random[[1]]$effects[[index]]);days<-seq(0,9,length.out=100)
  observed[[length(observed)+1]]<-data.frame(subject=paste('Subject',subject),days=rows$Days,reaction=rows$Reaction,series='Observed')
  lines[[length(lines)+1]]<-data.frame(subject=paste('Subject',subject),days=rep(days,2),reaction=c(beta[1]+beta[2]*days,beta[1]+b[1]+(beta[2]+b[2])*days),series=rep(c('Population mean','Subject conditional mean'),each=length(days)))
 }
 d<-bind(lines);obs<-bind(observed)
 p<-ggplot(d,aes(days,reaction,color=series,linetype=series))+geom_line(linewidth=.8)+geom_point(data=obs,aes(days,reaction,color=series),inherit.aes=FALSE,size=1.7)+
  scale_color_manual(values=c(Observed=unname(concept_colors['observed']),`Population mean`=unname(concept_colors['reference']),`Subject conditional mean`=unname(concept_colors['conditional'])),limits=c('Observed','Population mean','Subject conditional mean'))+
  scale_linetype_manual(values=c(Observed='blank',`Population mean`='dashed',`Subject conditional mean`='solid'),limits=c('Observed','Population mean','Subject conditional mean'))+facet_wrap(~subject,ncol=3)+
  labs(title='Random intercepts and slopes in the sleepstudy data',x='Days of sleep deprivation',y='Reaction time (ms)',caption='Published observations and the saved REML fit; population curves use zero random effects. Numerical agreement does not establish model adequacy.')
 save_figure(p,file.path(HERE,'sleepstudy-slopes'),11,6)
}
profile <- function(){
 path<-file.path(HERE,'data','lmm-profile.json');current_inputs<<-path;r<-read_json(path)
 d<-data.frame(x=v(r$curve),value=v(r$objective));zero<-data.frame(x=0,value=r$boundary)
 p<-ggplot(d,aes(x,value))+geom_line(color=unname(concept_colors['reference']))+geom_vline(xintercept=r$best_t,color=unname(concept_colors['conditional']),linetype='dashed')+
  geom_point(data=zero,aes(x,value),color=unname(concept_colors['approximation']),size=3)+
  labs(title='Variance-ratio profile on the teaching sample',subtitle='Dashed line: selected ML fit; orange point: zero group variance',x='log(1 + random/residual variance ratio)',y='Negative log-likelihood minus plotted minimum',caption='Points exported by the numerical walkthrough; figure rendering does not refit the model.')
 save_figure(p,file.path(HERE,'lmm-profile'),9,4.5)
}
posterior <- function(){
 path<-file.path(HERE,'data','logistic-mixed-posterior.json');current_inputs<<-path;r<-read_json(path)
 d<-data.frame(u=rep(v(r$u),2),density=c(v(r$actual),v(r$gaussian)),series=rep(c('Integrated conditional distribution','Gaussian approximation at the mode'),each=length(v(r$u))))
 d$series<-factor(d$series,levels=c('Integrated conditional distribution','Gaussian approximation at the mode'))
 p<-ggplot(d,aes(u,density,color=series,linetype=series))+geom_line(linewidth=.9)+geom_vline(xintercept=r$mode,color='#64748B')+
  scale_color_manual(values=c(`Integrated conditional distribution`=unname(concept_colors['reference']),`Gaussian approximation at the mode`=unname(concept_colors['approximation'])))+
  scale_linetype_manual(values=c(`Integrated conditional distribution`='solid',`Gaussian approximation at the mode`='dashed'))+
  labs(title=paste('Rare events: conditional distribution of group',r$group),subtitle='Vertical line: conditional mode',x='Standardized random intercept u (b = tau * u)',y='Conditional density',caption='Independently normalized points from the walkthrough; a Gaussian approximation can miss skewness and tail shape.')
 save_figure(p,file.path(HERE,'logistic-mixed-posterior'),10,4.8)
}
choices<-c('ols','logistic','probit','poisson','lmm','logistic-mixed','approximation','public-agreement','sleepstudy','profile','posterior')
jobs<-if(length(args)==0 || args[1]=='--all')choices else args
if(any(!jobs%in%choices))stop('Unknown figure job. Choose: ',paste(choices,collapse=', '))
for(job in jobs){
 if(job%in%choices[1:6])benchmark(job) else switch(job,approximation=approximation(),`public-agreement`=public_agreement(),sleepstudy=sleepstudy(),profile=profile(),posterior=posterior())
}
manifest$renderer_md5<-md5(file.path(HERE,'render.R'));manifest$theme_md5<-md5(file.path(HERE,'style.R'))
manifest$R<-R.version.string;manifest$platform<-R.version$platform
manifest$packages<-as.list(setNames(vapply(c('ggplot2','jsonlite','svglite'),function(p)as.character(packageVersion(p)),''),c('ggplot2','jsonlite','svglite')))
jsonlite::write_json(manifest,manifest_path,auto_unbox=TRUE,pretty=TRUE,null='null')
