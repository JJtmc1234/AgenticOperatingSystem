"""Evan's fixed manual and timer entry points."""
import argparse
import json
import os
from pathlib import Path
import shutil
import sys
from . import config, workflow


def main():
    parser=argparse.ArgumentParser(description='Evan governed issue fixer')
    parser.add_argument('--home',type=Path,default=Path.home()/'.carl/evan')
    commands=parser.add_subparsers(dest='command',required=True)
    run=commands.add_parser('run')
    run.add_argument('--repo')
    run.add_argument('--trigger',choices=['manual','poll'],default='manual')
    run.add_argument('--draft',action='store_true')
    run.add_argument('--retry',action='store_true',help='Retry an interrupted or blocked local attempt')
    commands.add_parser('status')
    commands.add_parser('doctor')
    args=parser.parse_args()
    os.umask(0o077)
    try:
        settings=config.load(args.home)
        if args.command=='status':
            path=args.home/'latest-report.json'
            print(path.read_text() if path.exists() else 'Evan has not run yet.')
            return 0
        if args.command=='doctor':
            missing=[p for p in ('git','gh','claude','bwrap') if not shutil.which(p)]
            if missing:
                raise RuntimeError('Missing programs: '+', '.join(missing))
            from .sandbox import run, passed
            if not passed(run({},[['/bin/true']],10)):
                raise RuntimeError('Test sandbox failed')
            print(json.dumps(dict(repositories=list(settings['repositories']),publish=settings['publish'],sandbox='available')))
            return 0
        if args.draft:
            settings['publish']=False
        report=workflow.run(args.home,settings,args.repo,args.trigger,args.retry)
        print(json.dumps(report,indent=2))
        return int(any(row['status'].startswith('Blocked.') for row in report['rows']))
    except Exception as error:
        print('Evan: '+str(error),file=sys.stderr)
        return 1


if __name__=='__main__':
    raise SystemExit(main())
