import {beforeEach, afterEach, it, expect, vi} from 'vitest';
const {invoke} = vi.hoisted(() => ({invoke:vi.fn()}));
vi.mock('@/lib/tauri', () => ({invoke,isTauriRuntime:()=>true}));
import {startSnapshotCacheMaintenance} from '@/stores/snapshotCacheMaintenance';
import {useRepositorySyncStore as sync} from '@/stores/repositorySyncStore';
import {useTaskQueueStore as queue,type BackgroundTask} from '@/stores/taskQueueStore';
let stop:()=>void;
const report=(id:string)=>({repositories:[{repository:'example/repo',snapshotId:id,added:[],modified:[],deleted:[],unchanged:[]}]});
const tick=()=>vi.advanceTimersByTimeAsync(300);
beforeEach(()=>{
 vi.useFakeTimers();invoke.mockReset();invoke.mockResolvedValue([]);
 sync.setState({preview:report('review'),isChecking:false,checkingRepository:null,migratingRepository:null});queue.setState({tasks:[]});
 vi.spyOn(sync.persist,'hasHydrated').mockReturnValue(true);vi.spyOn(queue.persist,'hasHydrated').mockReturnValue(true);
});
afterEach(()=>{stop?.();vi.restoreAllMocks();vi.useRealTimers();});
it('cleans startup orphans but retains current reviews and retryable applications',async()=>{
 const task:BackgroundTask={id:'retry',key:'retry',label:'retry',kind:'update',status:'failed',cancelRequested:false,createdAt:1,locks:[],steps:[{command:'apply_repository_snapshot',args:{snapshotId:'failed'},label:'retry'}]};
 queue.setState({tasks:[task]});invoke.mockResolvedValueOnce(['review','failed','orphan']);stop=startSnapshotCacheMaintenance();await tick();
 expect(invoke).toHaveBeenLastCalledWith('release_repository_snapshots',{snapshotIds:['orphan']});
});
it('waits for hydration instead of treating unloaded reports as unreferenced',async()=>{
 vi.mocked(sync.persist.hasHydrated).mockReturnValue(false);stop=startSnapshotCacheMaintenance();await tick();expect(invoke).not.toHaveBeenCalled();
 vi.mocked(sync.persist.hasHydrated).mockReturnValue(true);sync.setState({preview:report('loaded')});invoke.mockResolvedValueOnce(['loaded','orphan']);await tick();
 expect(invoke).toHaveBeenLastCalledWith('release_repository_snapshots',{snapshotIds:['orphan']});
});
it('rechecks live references after the inventory request completes',async()=>{
 let resolve!:(ids:string[])=>void;invoke.mockImplementationOnce(()=>new Promise<string[]>(r=>{resolve=r;}));stop=startSnapshotCacheMaintenance();await tick();
 sync.setState({preview:report('new-review')});resolve(['new-review','orphan']);await vi.advanceTimersByTimeAsync(0);
 expect(invoke).toHaveBeenLastCalledWith('release_repository_snapshots',{snapshotIds:['orphan']});
});
it('defers collection during checks and resumes after completion',async()=>{
 sync.setState({isChecking:true});stop=startSnapshotCacheMaintenance();await tick();expect(invoke).not.toHaveBeenCalled();
 invoke.mockResolvedValueOnce(['review','orphan']);sync.setState({isChecking:false});await tick();
 expect(invoke).toHaveBeenLastCalledWith('release_repository_snapshots',{snapshotIds:['orphan']});
});
it('abandons an inventory sweep if a check starts while it waits',async()=>{
 let resolve!:(ids:string[])=>void;invoke.mockImplementationOnce(()=>new Promise<string[]>(r=>{resolve=r;}));stop=startSnapshotCacheMaintenance();await tick();
 sync.setState({isChecking:true});resolve(['orphan']);await tick();expect(invoke).toHaveBeenCalledTimes(1);
});
it('sweeps when the report is cleared without waiting for another user check',async()=>{
 invoke.mockResolvedValueOnce(['review']);stop=startSnapshotCacheMaintenance();await tick();invoke.mockResolvedValueOnce(['review']);sync.setState({preview:null});await tick();
 expect(invoke).toHaveBeenLastCalledWith('release_repository_snapshots',{snapshotIds:['review']});
});
it('does not loop after cleanup errors and retries on the bounded timer',async()=>{
 invoke.mockRejectedValueOnce(new Error('unavailable'));stop=startSnapshotCacheMaintenance();await tick();await vi.advanceTimersByTimeAsync(1000);expect(invoke).toHaveBeenCalledTimes(1);
 invoke.mockResolvedValueOnce(['orphan']);await vi.advanceTimersByTimeAsync(300000);
 expect(invoke).toHaveBeenLastCalledWith('release_repository_snapshots',{snapshotIds:['orphan']});
});
it('does not delete anything after its owner has unmounted',async()=>{
 let resolve!:(ids:string[])=>void;invoke.mockImplementationOnce(()=>new Promise<string[]>(r=>{resolve=r;}));stop=startSnapshotCacheMaintenance();await tick();stop();resolve(['orphan']);await tick();expect(invoke).toHaveBeenCalledTimes(1);
});
