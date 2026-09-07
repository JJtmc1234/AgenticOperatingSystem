"""Manual and timer entry points for Iris."""
import argparse
import json
import os
from pathlib import Path
import sys
from . import config, workflow
from .ledger import Ledger


def main():
    parser=argparse.ArgumentParser(description='Iris repository investigation and issue creation')
    parser.add_argument('--home',type=Path,default=Path.home()/'.carl/iris')
    commands=parser.add_subparsers(dest='command',required=True)
    run=commands.add_parser('run')
    run.add_argument('--repo')
    run.add_argument('--request',default='')
    run.add_argument('--path',action='append',default=[],help='Review this exact committed file, repeat for related files')
    run.add_argument('--trigger',choices=['manual','hourly','feature','poll'],default='manual')
    run.add_argument('--force',action='store_true',help='Reinspect completed batches within the same budget')
    run.add_argument('--draft',action='store_true',help='Save local drafts instead of publishing')
    issue=commands.add_parser('issue',help='Verify and draft or publish a specific requested issue')
    issue.add_argument('--repo',required=True)
    issue.add_argument('--request',required=True,help='Problem or requested behavior, impact and expected outcome')
    issue.add_argument('--path',action='append',default=[],help='Exact committed file, repeat for related files')
    issue.add_argument('--draft',action='store_true')
    issue.set_defaults(trigger='manual',force=False)
    commands.add_parser('status')
    commands.add_parser('doctor')
    commands.add_parser('test',help='Run the installed local portal Playwright suite')
    args=parser.parse_args()
    os.umask(0o077)
    try:
        settings=config.load(args.home)
        if args.command=='test':
            from .browser import run as browser_test
            return browser_test(args.home)
        if args.command=='status':
            from .status import render
            print(render(args.home))
        elif args.command=='doctor':
            from shutil import which
            missing=[name for name in ['git','gh','claude'] if not which(name)]
            if missing:
                raise RuntimeError('Missing programs: '+', '.join(missing))
            print(json.dumps(dict(owner=settings['owner'],publish=settings['publish'],
                                  daily_budget_usd=settings['max_daily_usd'],tools='available')))
        else:
            if args.draft:
                settings['publish']=False
            result=workflow.run(args.home,settings,args.repo,args.request,args.trigger,args.force,
                                paths=args.path, specific=args.command=='issue')
            print((args.home/'latest-report.md').read_text())
            if any(row['status'].startswith('Failed:') for row in result['repositories']):
                return 1
        return 0
    except Exception as error:
        print('Iris: '+str(error),file=sys.stderr)
        return 1


if __name__=='__main__':
    raise SystemExit(main())
