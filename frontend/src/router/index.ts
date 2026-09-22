import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'home',
      component: () => import('@/views/HomeView.vue'),
    },
    {
      path: '/moment/:id',
      name: 'moment',
      component: () => import('@/views/MomentView.vue'),
      props: true,
    },
    {
      path: '/new',
      name: 'create',
      component: () => import('@/views/CreateView.vue'),
    },
    {
      path: '/gallery',
      name: 'gallery',
      component: () => import('@/views/GalleryView.vue'),
    },
    {
      path: '/search',
      name: 'search',
      component: () => import('@/views/SearchView.vue'),
    },
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue'),
    },
    {
      path: '/map',
      name: 'map',
      component: () => import('@/views/MapView.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/views/SettingsView.vue'),
    },
    {
      // 分享出去的链接：不需要登录，凭证就写在地址里
      path: '/s/:token',
      name: 'shared',
      component: () => import('@/views/SharedView.vue'),
      props: true,
    },
    {
      path: '/:pathMatch(.*)*',
      redirect: '/',
    },
  ],
  scrollBehavior: () => ({ top: 0 }),
})

/**
 * 每次切换页面之后，短暂吞掉浏览器"补发"的那一套鼠标事件。
 *
 * 手指点一下时，浏览器在 touchend 之后还会合成 mousedown / mouseup / click ——
 * 它们**发生在路由切换之后**，于是落在新页面同一坐标的元素上。要是那里正好是
 * 可点开大图的相册按钮，就会"顺手"把大图打开。
 *
 * 实测（手机尺寸、缓存好的第二次进入）：点一条有图的记忆，必然直接弹大图；
 * 而第一次进入往往躲得过——因为路由 chunk 还在下载、页面切得慢，那套事件打在旧页面上。
 * 这就是"有概率直接进大图"的来由。
 *
 * 吞掉的是导航后 300ms 内的这几种事件：人手不可能这么快在新页面上完成一次点击，
 * 而补发的那一套就在几毫秒之内。键盘、触摸（pointer/touch）都不受影响。
 */
router.afterEach(() => {
  const swallow = (event: Event) => {
    event.stopPropagation()
    event.preventDefault()
  }
  const options: AddEventListenerOptions = { capture: true }
  const types = ['mousedown', 'mouseup', 'click']
  for (const type of types) document.addEventListener(type, swallow, options)
  window.setTimeout(() => {
    for (const type of types) document.removeEventListener(type, swallow, options)
  }, 300)
})

export default router
