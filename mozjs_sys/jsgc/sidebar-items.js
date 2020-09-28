initSidebarItems({"struct":[["CustomAutoRooterVFTable","Methods for a CustomAutoRooter"],["Heap","Heap values encapsulate GC concerns of an on-heap reference to a JS object. This means that every reference to a JS object on heap must be realized through this structure."],["Rooted",""]],"trait":[["GCMethods","A trait for types which can place appropriate GC barriers. * https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/Internals/Garbage_collection#Incremental_marking * https://dxr.mozilla.org/mozilla-central/source/js/src/gc/Barrier.h"],["IntoHandle","Trait for things that can be converted to handles For any type `T: IntoHandle` we have an implementation of `From<T>` for `MutableHandle<T::Target>`. This is a way round the orphan rule."],["IntoMutableHandle",""],["RootKind","A trait for JS types that can be registered as roots."]]});