<template>
  <NuxtLoadingIndicator />
  <NuxtLayout>
    <nav style="display: flex; gap: 10px">
      <NuxtLink to="/">Home</NuxtLink>
      <NuxtLink to="/search">Search</NuxtLink>
    </nav>
    <NuxtPage />
  </NuxtLayout>
</template>
