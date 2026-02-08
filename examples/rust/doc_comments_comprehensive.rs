//! A crate-level doc comment that applies to the implicit anonymous module of this crate
//! This should be attached to the crate/file-level module symbol
//! FINAL_VERIFICATION_TEST - confirming SymbolId synchronization works

pub mod outer_module {
    //!  - Inner line doc for outer_module
    //!! - Still an inner line doc (but with a bang at the beginning)

    /*!  - Inner block doc for outer_module */
    /*!! - Still an inner block doc (but with a bang at the beginning) */

    //   - Only a comment (not a doc comment)
    ///  - Outer line doc for the next item (exactly 3 slashes)
    //// - Only a comment (4 slashes, not a doc comment)

    /*   - Only a comment (not a doc comment) */
    /**  - Outer block doc for the next item (exactly 2 asterisks) */
    /*** - Only a comment (3+ asterisks, not a doc comment) */

    pub mod inner_module {
        //! Inner doc comment for inner_module
    }

    /// This is proper outer line documentation for documented_function
    /// It spans multiple lines and should be collected together
    /// Each line starts with exactly three slashes
    pub fn documented_function() {
        // Regular comment inside function
    }

    /** 
     * This is proper outer block documentation for block_documented_function
     * It uses the standard block comment format
     * With asterisks for formatting
     */
    pub fn block_documented_function() {
        /* Regular block comment inside function */
    }

    //// This is NOT a doc comment (4 slashes)
    pub fn not_doc_commented_function() {}

    /*** This is NOT a doc comment (3 asterisks) */  
    pub fn another_not_doc_commented_function() {}

    pub mod nested_comments {
        /* In Rust /* we can /* nest comments */ */ */

        // All three types of block comments can contain or be nested inside
        // any other type:

        /*   /* */  /** */  /*! */  */
        /*!  /* */  /** */  /*! */  */
        /**  /* */  /** */  /*! */  */
        
        /// Outer doc comment for dummy_item in nested context
        pub mod dummy_item {}
    }

    /// HOT_RELOAD_SUCCESS_TEST: Real-time embedding updates working perfectly
    /// MEMORY_OPTIMIZATION_COMPLETE: SymbolId synchronization fix verified in production
    pub struct SYMBOL_CHANGE_TEST_Struct {
        //! Inner documentation for the struct itself
        //! This describes the struct from the inside
        
        /// Documentation for a field
        pub documented_field: String,
        
        // Regular comment for a field
        pub regular_field: i32,
    }

    /// Documentation for a trait
    pub trait DocumentedTrait {
        //! Inner documentation for the trait
        
        /// Documentation for an associated function
        fn documented_method(&self);
        
        // Regular comment
        fn regular_method(&self);
    }

    /// Implementation documentation
    impl DocumentedTrait for DocumentedStruct {
        //! Inner doc for the impl block
        
        /// Method implementation documentation
        fn documented_method(&self) {
            // Regular implementation comment
        }
        
        fn regular_method(&self) {
            // Regular implementation
        }
    }

    /// Documentation for an enum
    pub enum DocumentedEnum {
        //! Inner documentation for the enum
        
        /// Documentation for a variant
        DocumentedVariant,
        
        // Regular comment
        RegularVariant,
        
        /// Documentation for a variant with fields
        DocumentedVariantWithFields {
            /// Field documentation
            documented_field: String,
            // Regular field comment
            regular_field: i32,
        }
    }

    pub mod degenerate_cases {
        // empty inner line doc
        //!

        // empty inner block doc
        /*!*/

        // empty line comment
        //

        /// empty outer line doc
        ///

        // empty block comment
        /**/

        /// Empty outer line doc for dummy item
        pub mod dummy_item {}

        // empty 2-asterisk block isn't a doc block, it is a block comment
        /***/
        pub fn after_empty_block() {}
    }

    // Complex case: multiple comment blocks before one item
    /// First documentation block
    /// Multiple lines in first block
    
    /// Second documentation block  
    /// This should also be collected
    
    /** 
     * Third block in different format
     * Should be combined with the line comments above
     */
    pub fn multiple_comment_blocks() {}

    // Edge case: mixed valid and invalid doc comments
    //// Not a doc comment (4 slashes)
    /// Valid doc comment
    /*** Not a doc comment (3 asterisks) */
    /** Valid block doc comment */
    pub fn mixed_comment_types() {}

    pub mod export_awareness_tests {
        /// This should be found even with pub mod
        pub mod public_module {}
        
        /// This should be found with pub fn  
        pub fn public_function() {}
        
        /// This should be found with pub struct
        pub struct PublicStruct {}
        
        /// This should be found with pub enum
        pub enum PublicEnum {
            Variant
        }
        
        /// This should be found with pub trait
        pub trait PublicTrait {}
        
        /// Private function documentation
        fn private_function() {}
        
        // Test complex pub patterns
        /// Documentation for pub(crate) function
        pub(crate) fn crate_visible_function() {}
        
        /// Documentation for pub(super) function  
        pub(super) fn super_visible_function() {}
        
        /// Documentation for pub(in path) function
        pub(in crate::outer_module) fn path_visible_function() {}
    }
}

/// Documentation at the end of file
pub fn final_documented_function() {
    //! This inner doc should attach to final_documented_function
}// trigger reindex
// trigger reindex
// trigger reindex again
// trigger reindex again
