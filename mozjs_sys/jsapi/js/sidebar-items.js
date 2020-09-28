initSidebarItems({"constant":[["ClassSpec_DontDefineConstructor",""],["ClassSpec_ProtoKeyMask",""],["ClassSpec_ProtoKeyWidth",""],["ProfilingStackFrame_NullPCOffset",""]],"enum":[["AllocFunction",""],["AllowGC","Types for a variable that either should or shouldn't be rooted, depending on the template parameter allowGC. Used for implementing functions that can operate on either rooted or unrooted data."],["CTypesActivityType",""],["CompletionKind",""],["DOMProxyShadowsResult",""],["DumpHeapNurseryBehaviour",""],["ESClass","Enumeration describing possible values of the [[Class]] internal property value of objects."],["ElementAdder_GetBehavior",""],["ErrorReport_SniffingBehavior",""],["NukeReferencesFromTarget",""],["NukeReferencesToWindow",""],["ProfilingStackFrame_Flags",""],["StackFormat",""],["ThreadType",""]],"fn":[["AddListFormatConstructor",""],["AddLocaleConstructor",""],["AddMozDateTimeFormatConstructor",""],["AddRawValueRoot",""],["AllowNewWrapper",""],["AppendUnique",""],["AreGCGrayBitsValid",""],["AssertCompartmentHasSingleRealm",""],["AssertHeapIsIdle",""],["AssertJSStringBufferInCorrectArena",""],["AssertSameCompartment",""],["AssertSameCompartment1",""],["AssertSameCompartment2",""],["CheckGrayMarkingState",""],["CompartmentHasLiveGlobal",""],["ConvertArgsToArray",""],["CurrentThreadCanAccessRuntime",""],["CurrentThreadCanAccessZone",""],["CurrentThreadIsPerformingGC",""],["DateGetMsecSinceEpoch",""],["DateIsValid","Detect whether the internal date value is NaN."],["DefineFunctionWithReserved",""],["DumpAtom",""],["DumpAtom1",""],["DumpBacktrace",""],["DumpBacktrace1",""],["DumpChars",""],["DumpChars1",""],["DumpHeap","Dump the complete object graph of heap-allocated things. fp is the file for the dump output."],["DumpId",""],["DumpId1",""],["DumpInterpreterFrame",""],["DumpInterpreterFrame1",""],["DumpObject",""],["DumpObject1",""],["DumpPC",""],["DumpPC1",""],["DumpScript",""],["DumpScript1",""],["DumpString",""],["DumpString1",""],["DumpValue",""],["DumpValue1",""],["EnableAccessValidation",""],["EnableContextProfilingStack",""],["EnqueueJob","Enqueue |job| on the internal job queue."],["ExecuteInFrameScriptEnvironment",""],["ExecuteInJSMEnvironment",""],["ExecuteInJSMEnvironment1",""],["ForwardToNative",""],["FunctionHasNativeReserved",""],["GetAllocationMetadata","Get the metadata associated with an object."],["GetAnyRealmInZone",""],["GetArrayBufferViewLengthAndData",""],["GetBuiltinClass",""],["GetCodeCoverageSummary","Generate lcov trace file content for the current realm, and allocate a new buffer and return the content in it, the size of the newly allocated content within the buffer would be set to the length out-param. The 'All' variant will collect data for all realms in the runtime."],["GetCodeCoverageSummaryAll",""],["GetDOMCallbacks",""],["GetDOMProxyHandlerFamily",""],["GetDOMProxyShadowsCheck",""],["GetDOMRemoteProxyHandlerFamily",""],["GetElementsWithAdder",""],["GetErrorMessage",""],["GetErrorTypeName","Get an error type name from a JSExnType constant. Returns nullptr for invalid arguments and JSEXN_INTERNALERR"],["GetFirstGlobalInCompartment",""],["GetFirstSubsumedSavedFrame","Get the first SavedFrame object in this SavedFrame stack whose principals are subsumed by the given |principals|. If there is no such frame, return nullptr."],["GetFunctionNativeReserved",""],["GetGCHeapUsageForObjectZone","This function only reports GC heap memory, and not malloc allocated memory associated with GC things."],["GetJSMEnvironmentOfScriptedCaller",""],["GetObjectProto",""],["GetObjectSlotSpan",""],["GetPCCountScriptContents",""],["GetPCCountScriptCount",""],["GetPCCountScriptSummary",""],["GetPropertyKeys","Add some or all property keys of obj to the id vector *props."],["GetPropertyNameFromPC",""],["GetPrototypeNoProxy",""],["GetRealmOriginalEval",""],["GetRealmZone",""],["GetSCOffset",""],["GetScriptRealm",""],["GetStackFormat",""],["GetStaticPrototype",""],["GetTestingFunctions",""],["GetXrayJitInfo",""],["GlobalHasInstrumentation","Return whether a global object's realm has had instrumentation enabled by a Debugger."],["HasObjectMovedOp",""],["InitMallocAllocator",""],["IsArgumentsObject",""],["IsAtomsZone",""],["IsCompartmentZoneSweepingOrCompacting",""],["IsDOMRemoteProxyObject",""],["IsFunctionObject",""],["IsJSMEnvironment",""],["IsObjectInContextCompartment",""],["IsSharableCompartment",""],["IsSystemCompartment",""],["IsSystemRealm",""],["IsSystemZone",""],["IsWindowProxy","Returns true iff `obj` has the WindowProxy Class (see SetWindowProxyClass)."],["IterateGrayObjects","Invoke cellCallback on every gray JSObject in the given zone."],["IterateGrayObjectsUnderCC","Invoke cellCallback on every gray JSObject in the given zone while cycle collection is in progress."],["LogCtor",""],["LogDtor",""],["MaybeGetScriptPrivate","Get the script private value associated with an object, if any."],["MemoryReportingSundriesThreshold","In memory reporting, we have concept of \"sundries\", line items which are too small to be worth reporting individually.  Under some circumstances, a memory reporter gets tossed into the sundries bucket if it's smaller than MemoryReportingSundriesThreshold() bytes."],["NewFunctionByIdWithReserved",""],["NewFunctionWithReserved",""],["NewJSMEnvironment",""],["NoteIntentionalCrash","Hint that we expect a crash. Currently, the only thing that cares is the breakpad injector, which (if loaded) will suppress minidump generation."],["NotifyAnimationActivity",""],["NukeCrossCompartmentWrappers",""],["NukedObjectRealm",""],["ObjectClassName",""],["PrepareScriptEnvironmentAndInvoke",""],["ProtoKeyToClass",""],["PurgePCCounts",""],["RegExpToSharedNonInline",""],["RegisterContextProfilingEventMarker",""],["RemapRemoteWindowProxies",""],["RemoveRawValueRoot",""],["ReportIsNotFunction",""],["ReportOutOfMemory",""],["ReportOverRecursed",""],["RunJobs",""],["RunningWithTrustedPrincipals",""],["RuntimeIsBeingDestroyed",""],["SetAllocationMetadataBuilder","Specify a callback to invoke when creating each JS object in the current compartment, which may return a metadata object to associate with the object."],["SetCTypesActivityCallback","Sets a callback that is run whenever js-ctypes is about to be used when calling into C."],["SetContextProfilingStack",""],["SetDOMCallbacks",""],["SetDOMProxyInformation",""],["SetFunctionNativeReserved",""],["SetLogCtorDtorFunctions","Set global function used to monitor a few internal classes to highlight leaks, and to hint at the origin of the leaks."],["SetPreserveWrapperCallback",""],["SetPropertyIgnoringNamedGetter","Helper function for HTMLDocument and HTMLFormElement."],["SetRealmValidAccessPtr",""],["SetReservedSlotWithBarrier",""],["SetScriptEnvironmentPreparer",""],["SetStackFormat",""],["SetWindowProxy","Associates a WindowProxy with a Window (global object). `windowProxy` must have the Class set by SetWindowProxyClass."],["SetWindowProxyClass","Tell the JS engine which Class is used for WindowProxy objects. Used by the functions below."],["SetXrayJitInfo",""],["ShouldIgnorePropertyDefinition",""],["ShutDownMallocAllocator",""],["StartPCCountProfiling",""],["StopDrainingJobQueue","Instruct the runtime to stop draining the internal job queue."],["StopPCCountProfiling",""],["StringIsArrayIndex","Determine whether the given string is an array index in the sense of https://tc39.github.io/ecma262/#array-index."],["StringIsArrayIndex1","Overloads of StringIsArrayIndex taking (char*,length) pairs.  These behave the same as the JSLinearString version."],["StringIsArrayIndex2",""],["StringToLinearStringSlow",""],["SystemZoneAvailable",""],["ToBooleanSlow",""],["ToInt16Slow",""],["ToInt32Slow",""],["ToInt64Slow",""],["ToInt8Slow",""],["ToNumberSlow",""],["ToObjectSlow",""],["ToStringSlow",""],["ToUint16Slow",""],["ToUint32Slow",""],["ToUint64Slow",""],["ToUint8Slow",""],["ToWindowIfWindowProxy","If `obj` is a WindowProxy, get its associated Window (the compartment's global), else return `obj`. This function is infallible and never returns nullptr."],["TraceWeakMaps",""],["Unbox",""],["UninlinedIsCrossCompartmentWrapper",""],["UnwrapArrayBufferView",""],["UnwrapBigInt64Array",""],["UnwrapBigUint64Array",""],["UnwrapFloat32Array",""],["UnwrapFloat64Array",""],["UnwrapInt16Array",""],["UnwrapInt32Array",""],["UnwrapInt8Array",""],["UnwrapReadableStream",""],["UnwrapUint16Array",""],["UnwrapUint32Array",""],["UnwrapUint8Array",""],["UnwrapUint8ClampedArray",""],["UseInternalJobQueues","Use the runtime's internal handling of job queues for Promise jobs."],["VisitGrayWrapperTargets",""],["ZoneGlobalsAreAllGray",""]],"mod":[["Scalar",""],["detail",""],["gc",""],["gcstats",""],["jit",""],["oom",""],["shadow",""]],"static":[["AutoEnterOOMUnsafeRegion_annotateOOMSizeCallback",""],["AutoEnterOOMUnsafeRegion_owner_",""]],"struct":[["AllCompartments",""],["AllocPolicyBase",""],["AllocationMetadataBuilder",""],["AllocationMetadataBuilder__bindgen_vtable",""],["AtomicRefCounted",""],["AutoAssertNoContentJS",""],["AutoCTypesActivityCallback",""],["AutoEnterOOMUnsafeRegion",""],["AutoGeckoProfilerEntry",""],["BarrierMethods",""],["BarrieredBase",""],["BaseProxyHandler",""],["BaseShape",""],["BufferIterator",""],["ChromeCompartmentsOnly",""],["ClassExtension",""],["ClassSpec",""],["CompartmentFilter",""],["CompartmentFilter__bindgen_vtable",""],["CompartmentTransplantCallback",""],["CompartmentTransplantCallback__bindgen_vtable",""],["ContentCompartmentsOnly",""],["DispatchWrapper",""],["ElementAdder",""],["ErrorReport",""],["ExpandoAndGeneration",""],["FakeMutableHandle",""],["FakeRooted",""],["GeckoProfilerBaselineOSRMarker",""],["GeckoProfilerEntryMarker",""],["GeckoProfilerThread",""],["HandleBase",""],["HeapBase",""],["InefficientNonFlatteningStringHashPolicy","This hash policy avoids flattening ropes (which perturbs the site being measured and requires a JSContext) at the expense of doing a FULL ROPE COPY on every hash and match! Beware."],["InterpreterFrame",""],["IsHeapConstructibleType",""],["JSDOMCallbacks",""],["LazyScript",""],["MovableCellHasher",""],["MutableHandleBase",""],["MutableValueOperations",""],["MutableWrappedPtrOperations",""],["ObjectGroup",""],["ObjectOps",""],["PersistentRootedBase",""],["ProfilingStackFrame",""],["RefCounted",""],["RegExpShared",""],["RootedBase",""],["RunnableTask",""],["RunnableTask__bindgen_vtable",""],["Scope",""],["ScriptEnvironmentPreparer","PrepareScriptEnvironmentAndInvoke asserts the embedder has registered a ScriptEnvironmentPreparer and then it calls the preparer's 'invoke' method with the given |closure|, with the assumption that the preparer will set up any state necessary to run script in |global|, invoke |closure| with a valid JSContext*, report any exceptions thrown from the closure, and return."],["ScriptEnvironmentPreparer_Closure",""],["ScriptEnvironmentPreparer_Closure__bindgen_vtable",""],["ScriptEnvironmentPreparer__bindgen_vtable",""],["ScriptSource",""],["Shape",""],["SharedArrayRawBuffer",""],["SharedArrayRawBufferRefs",""],["SingleCompartment",""],["SystemAllocPolicy",""],["TempAllocPolicy",""],["WeakMapTracer",""],["WeakMapTracer__bindgen_vtable",""],["WrappedPtrOperations",""],["XrayJitInfo",""]],"type":[["AutoEnterOOMUnsafeRegion_AnnotateOOMAllocationSizeCallback",""],["CTypesActivityCallback",""],["ClassObjectCreationOp","Callback for the creation of constructor and prototype objects."],["DOMCallbacks",""],["DOMInstanceClassHasProtoAtDepth",""],["DOMProxyShadowsCheck",""],["DefaultHasher",""],["DefinePropertyOp",""],["DeletePropertyOp",""],["DispatchWrapper_TraceFn",""],["FinishClassInitOp","Callback for custom post-processing after class initialization via ClassSpec."],["GCThingCallback",""],["GetElementsOp",""],["GetOwnPropertyOp",""],["GetPropertyOp",""],["HasPropertyOp",""],["HashMap",""],["HashNumber",""],["HashSet",""],["InefficientNonFlatteningStringHashPolicy_Lookup",""],["LogCtorDtor",""],["LookupPropertyOp",""],["MovableCellHasher_Key",""],["MovableCellHasher_Lookup",""],["PointerHasher",""],["PreserveWrapperCallback",""],["SetPropertyOp",""],["UniquePtr",""],["Vector",""]]});