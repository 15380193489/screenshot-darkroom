import {execFileSync} from 'node:child_process';

const git=(...args)=>execFileSync('git',args,{encoding:'utf8'}).trim();
const allowed=email=>/^[^<>\s]+@users\.noreply\.github\.com$/i.test(email)||email==='noreply@github.com';
let emails;
if(process.argv.includes('--all')){
  emails=git('log','--all','--format=%ae%n%ce','--no-use-mailmap').split(/\r?\n/).filter(Boolean);
}else{
  emails=['GIT_AUTHOR_IDENT','GIT_COMMITTER_IDENT'].map(kind=>{
    const match=git('var',kind).match(/<([^<>]+)>/);
    if(!match)throw Error('Cannot read the Git commit identity.');
    return match[1];
  });
}
const count=emails.filter(email=>!allowed(email)).length;
if(count){
  console.error(`Commit privacy check failed: ${count} author/committer entries use a personal email. Email values are intentionally omitted.`);
  console.error('Configure your GitHub noreply email before committing. Existing commits need separate history cleanup.');
  process.exitCode=1;
}else console.log('Commit privacy check passed: only GitHub noreply addresses were found.');
