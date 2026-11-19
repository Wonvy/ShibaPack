import { createRouter, createWebHashHistory } from "vue-router";
import LibraryView from "../views/LibraryView.vue";
import InstalledView from "../views/InstalledView.vue";
import TagsView from "../views/TagsView.vue";
import StorageView from "../views/StorageView.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/library" },
    { path: "/library", name: "library", component: LibraryView },
    { path: "/installed", name: "installed", component: InstalledView },
    { path: "/tags", name: "tags", component: TagsView },
    { path: "/storage", name: "storage", component: StorageView },
  ],
});

export default router;
