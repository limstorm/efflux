import { createApp } from 'vue'
import { createPinia } from 'pinia'

import '@fontsource-variable/noto-serif-sc'
import '@fontsource/cormorant-garamond/400.css'
import '@fontsource/cormorant-garamond/600.css'
import './styles/main.css'

import App from './App.vue'
import router from './router'

createApp(App).use(createPinia()).use(router).mount('#app')
