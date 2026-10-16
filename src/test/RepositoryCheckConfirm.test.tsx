import {beforeEach,it,expect,vi} from "vitest";
import {render,screen,fireEvent} from "@testing-library/react";
import {RepositoryCheckConfirm} from "@/components/skill/RepositoryCheckConfirm";
import {useRepositorySyncStore as store} from "@/stores/repositorySyncStore";
beforeEach(()=>{vi.restoreAllMocks();store.setState({isChecking:false,checkingRepository:null,preview:null,open:false});});
it("starts a check only after explicit confirmation",()=>{
  const check=vi.spyOn(store.getState(),"checkForUpdates").mockResolvedValue();
  const close=vi.fn();
  render(<RepositoryCheckConfirm open onOpenChange={close}/>);
  expect(check).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"取消"}));
  expect(check).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"检查更新"}));
  expect(check).toHaveBeenCalledExactlyOnceWith(undefined);
});
it("opens saved results without scanning or checking",()=>{
  store.setState({preview:{repositories:[]}});
  const check=vi.spyOn(store.getState(),"checkForUpdates").mockResolvedValue();
  render(<RepositoryCheckConfirm open onOpenChange={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"上次结果"}));
  expect(store.getState().open).toBe(true);
  expect(check).not.toHaveBeenCalled();
});
it("keeps four icon actions in order and refreshes stars independently",()=>{
  const refresh=vi.spyOn(store.getState(),"refreshRepositories").mockResolvedValue();
  const check=vi.spyOn(store.getState(),"checkForUpdates").mockResolvedValue();
  const {container}=render(<RepositoryCheckConfirm open onOpenChange={vi.fn()}/>);
  const footer=document.querySelector('[data-slot="dialog-footer"]')!;
  expect(Array.from(footer.querySelectorAll('button')).map(button=>button.textContent)).toEqual(["上次结果","检查更新","更新仓库","取消"]);
  expect(footer.querySelectorAll('button svg')).toHaveLength(4);
  expect(screen.getByRole("button",{name:"上次结果"})).toBeDisabled();
  expect(screen.getByRole("heading",{name:"更新技能"}).querySelector('svg')).not.toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"更新仓库"}));
  expect(refresh).toHaveBeenCalledExactlyOnceWith(undefined);
  expect(check).not.toHaveBeenCalled();
  expect(container).toBeInTheDocument();
});
