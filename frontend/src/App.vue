<script setup lang="ts">
import ToastHost from '@/components/ToastHost.vue'
</script>

<template>
  <div class="app-shell">
    <div class="ambient" aria-hidden="true" />
    <div class="vignette" aria-hidden="true" />

    <RouterView v-slot="{ Component }">
      <Transition name="page">
        <KeepAlive include="HomeView">
          <component :is="Component" />
        </KeepAlive>
      </Transition>
    </RouterView>

    <div class="grain" aria-hidden="true" />
    <ToastHost />
  </div>
</template>
