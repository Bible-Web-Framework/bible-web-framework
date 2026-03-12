<template>
  <NuxtLoadingIndicator />
  <NuxtLayout>
    <nav style="display: flex; gap: 10px">
      <NuxtLink to="/">Home</NuxtLink>
      <NuxtLink to="/search">Search</NuxtLink>
    </nav>
    <NuxtPage class="centered-body" />
  </NuxtLayout>
</template>

<style>
@import url('~/assets/bwf.css');
</style>
