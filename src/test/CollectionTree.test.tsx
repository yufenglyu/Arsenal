import {beforeEach,expect,it,vi} from 'vitest';
import {render,screen,fireEvent,within,createEvent,act} from '@testing-library/react';
import {MemoryRouter} from 'react-router-dom';
import {CollectionTree} from '@/components/collection/CollectionTree';
import {useCollectionStore as store} from '@/stores/collectionStore';
import {useResourceLibraryStore as resources} from '@/stores/resourceLibraryStore';
import {COLLECTION_DRAG,SKILLS_DRAG,REPOSITORIES_DRAG,COLLECTION_SOURCE_DRAG} from '@/lib/collectionGroups';
const change=vi.fn().mockResolvedValue(undefined);
const groups=[{id:'a',name:'Research',parent_id:null,created_at:'',updated_at:''},{id:'b',name:'Writing',parent_id:'a',created_at:'',updated_at:''},{id:'c',name:'Other',parent_id:null,created_at:'',updated_at:''}];
beforeEach(()=>{localStorage.clear();vi.clearAllMocks();resources.setState({skills:[]});store.setState({collections:groups,groupDetails:[],selectedSkillIds:[],activeCollectionId:null,includeDescendants:true,loadGroupDetails:vi.fn().mockResolvedValue(undefined),changeGroup:change});});
function setup(){render(<MemoryRouter><CollectionTree/></MemoryRouter>);}
function transfer(values:Record<string,string>){return {types:Object.keys(values),getData:(key:string)=>values[key]??'',setData:vi.fn(),effectAllowed:'',dropEffect:''};}
it('selects groups in the repository and navigates visible rows with arrows',()=>{
 setup();const first=screen.getByRole('button',{name:'Research 0/0'});fireEvent.click(first);expect(store.getState().activeCollectionId).toBe('a');
 fireEvent.keyDown(first,{key:'ArrowDown'});expect(store.getState().activeCollectionId).toBe('b');expect(screen.getByRole('button',{name:'Writing 0/0'})).toHaveFocus();
 fireEvent.keyDown(first,{key:'ArrowLeft'});expect(screen.queryByRole('button',{name:'Writing 0/0'})).not.toBeInTheDocument();
 fireEvent.keyDown(first,{key:'ArrowRight'});expect(screen.getByRole('button',{name:'Writing 0/0'})).toBeInTheDocument();
});
it('collapses and expands child groups while keeping the repository root open',()=>{
 setup();const first=screen.getByRole('button',{name:'Research 0/0'});fireEvent.keyDown(first,{key:'-'});expect(screen.getAllByRole('treeitem')).toHaveLength(2); expect(screen.getByText('未分组')).toBeInTheDocument();
 fireEvent.keyDown(screen.getByRole('tree'),{key:'+'});expect(screen.getAllByRole('treeitem')).toHaveLength(3);
});
it('opens the keyboard context menu and creates a child at the chosen parent',async()=>{
 setup();fireEvent.keyDown(screen.getByRole('button',{name:'Research 0/0'}),{key:'F10',shiftKey:true});
 fireEvent.click(screen.getByRole('menuitem',{name:'新建子分组'}));fireEvent.change(screen.getByRole('textbox',{name:'分组名称'}),{target:{value:'New child'}});fireEvent.click(screen.getByRole('button',{name:'确认'}));
 expect(change).toHaveBeenCalledWith(expect.objectContaining({action:'create',name:'New child',parentId:'a'}));await act(async()=>{});
});
it('adds dragged skills and transfers only on Shift drag with a source group',()=>{
 setup();const target=screen.getByRole('button',{name:'Other 0/0'}).parentElement!;
 const data=transfer({[SKILLS_DRAG]:'["one","two"]',[COLLECTION_SOURCE_DRAG]:'a'});
 fireEvent.drop(target,{dataTransfer:data});expect(change).toHaveBeenLastCalledWith(expect.objectContaining({action:'assign',id:'c',skillIds:['one','two'],sourceCollectionId:undefined}));
 const drop=createEvent.drop(target,{dataTransfer:data});Object.defineProperty(drop,'shiftKey',{value:true});fireEvent(target,drop);expect(change).toHaveBeenLastCalledWith(expect.objectContaining({action:'transfer',id:'c',sourceCollectionId:'a'}));
});
it('moves groups inside another group and requires confirmation before subtree deletion',async()=>{
 setup();const target=screen.getByRole('button',{name:'Other 0/0'}).parentElement!;
 fireEvent.drop(target,{dataTransfer:transfer({[COLLECTION_DRAG]:'a'})});expect(change).toHaveBeenLastCalledWith(expect.objectContaining({action:'move',id:'a',parentId:'c'}));change.mockClear();
 fireEvent.contextMenu(target);fireEvent.click(screen.getByRole('menuitem',{name:'删除分组'}));expect(change).not.toHaveBeenCalled();
 const dialog=screen.getByRole('dialog');expect(within(dialog).getByText(/技能文件保留/)).toBeInTheDocument();fireEvent.click(within(dialog).getByRole('button',{name:'确认'}));expect(change).toHaveBeenCalledWith(expect.objectContaining({action:'delete',id:'c'}));await act(async()=>{});
});

it('uses stable repository selectors instead of stale member IDs on drop',()=>{
 setup();const target=screen.getByRole('button',{name:'Other 0/0'}).parentElement!;
 const data=transfer({[SKILLS_DRAG]:'["obsolete"]',[REPOSITORIES_DRAG]:JSON.stringify({repositories:['example/research'],skillIds:['explicit']})});
 fireEvent.drop(target,{dataTransfer:data});
 expect(change).toHaveBeenCalledWith(expect.objectContaining({action:'assign',id:'c',repositories:['example/research'],skillIds:['explicit']}));
});
it('rejects malformed repository metadata without applying stale skill IDs',()=>{
 setup();const target=screen.getByRole('button',{name:'Other 0/0'}).parentElement!;
 fireEvent.drop(target,{dataTransfer:transfer({[SKILLS_DRAG]:'["obsolete"]',[REPOSITORIES_DRAG]:'{"repositories":[3],"skillIds":[]}'})});
 expect(change).not.toHaveBeenCalled();
});

it('counts distinct repositories and skills across subgroups with matching right padding',()=>{
 const one={id:'one',name:'One',file_path:'/library/example/tools/one/SKILL.md',source_repo:'Example/Tools',is_central:false,scanned_at:'',linked_agents:[],read_only_agents:[]};
 const two={...one,id:'two',name:'Two',source_repo:'EXAMPLE/TOOLS',file_path:'/library/example/tools/two/SKILL.md'};
 const three={...one,id:'three',name:'Three',source_repo:'Example/Other',file_path:'/library/example/other/three/SKILL.md'};
 resources.setState({skills:[one,two,three],resourceLibraryDir:'/library'});
 store.setState({groupDetails:[{...groups[0],skills:[one]},{...groups[1],skills:[one,two]}]});
 setup();
 const root=screen.getByRole('button',{name:'技能仓库'});
 expect(root).toHaveTextContent('2/3');expect(root).toHaveClass('pr-2');
 expect(within(root).getByTitle('2 个仓库 / 3 个技能')).toBeInTheDocument();
 expect(screen.getByRole('button',{name:'Research 1/2'})).toHaveClass('pr-2');
 expect(screen.getByRole('button',{name:'Writing 1/2'})).toHaveClass('pr-2');
 expect(screen.getByRole('button',{name:'未分组 1/1'})).toHaveClass('pr-2');
 act(()=>store.setState({includeDescendants:false}));
 expect(screen.getByRole('button',{name:'Research 1/1'})).toBeInTheDocument();
});
