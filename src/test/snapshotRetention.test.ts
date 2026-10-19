import {it,expect} from 'vitest';
import {retainedSnapshotIds,useRepositorySyncStore as sync} from '@/stores/repositorySyncStore';
import {useTaskQueueStore as queue,type BackgroundTask} from '@/stores/taskQueueStore';
it('retains the current report and retryable apply snapshots without retaining obsolete checks',()=>{
 sync.setState({preview:{repositories:[{repository:'example/repo',snapshotId:'reviewed',added:[],modified:[],deleted:[],unchanged:[]}]}});
 const task=(id:string,status:BackgroundTask['status']):BackgroundTask=>({id,key:id,label:id,kind:'update',status,locks:[],createdAt:1,cancelRequested:false,steps:[{command:'apply_repository_snapshot',label:id,args:{snapshotId:id}}]});
 queue.setState({tasks:[task('queued','queued'),task('failed','failed'),task('done','success'),{...task('check','success'),steps:[{command:'preview_source_backed_resource_repository_updates',label:'check',args:{},result:{repositories:[{snapshotId:'old'}]}}]}]});
 expect(retainedSnapshotIds().sort()).toEqual(['failed','queued','reviewed']);
 sync.setState({preview:null});queue.setState({tasks:[]});
});
