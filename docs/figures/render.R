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
choices<-c('profile','posterior')
jobs<-if(length(args)==0 || args[1]=='--all')choices else args
if(any(!jobs%in%choices))stop('Unknown figure job. Choose: ',paste(choices,collapse=', '))
for(job in jobs){
 switch(job,profile=profile(),posterior=posterior())
}
manifest$renderer_md5<-md5(file.path(HERE,'render.R'));manifest$theme_md5<-md5(file.path(HERE,'style.R'))
manifest$R<-R.version.string;manifest$platform<-R.version$platform
manifest$packages<-as.list(setNames(vapply(c('ggplot2','jsonlite','svglite'),function(p)as.character(packageVersion(p)),''),c('ggplot2','jsonlite','svglite')))
jsonlite::write_json(manifest,manifest_path,auto_unbox=TRUE,pretty=TRUE,null='null')
