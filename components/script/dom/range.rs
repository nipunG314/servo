/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::dom::abstractrange::{bp_position, AbstractRange};
use crate::dom::bindings::codegen::Bindings::AbstractRangeBinding::AbstractRangeMethods;
use crate::dom::bindings::codegen::Bindings::CharacterDataBinding::CharacterDataMethods;
use crate::dom::bindings::codegen::Bindings::NodeBinding::NodeMethods;
use crate::dom::bindings::codegen::Bindings::NodeListBinding::NodeListMethods;
use crate::dom::bindings::codegen::Bindings::RangeBinding::RangeMethods;
use crate::dom::bindings::codegen::Bindings::RangeBinding::{self, RangeConstants};
use crate::dom::bindings::codegen::Bindings::TextBinding::TextMethods;
use crate::dom::bindings::codegen::Bindings::WindowBinding::WindowMethods;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::inheritance::{CharacterDataTypeId, NodeTypeId};
use crate::dom::bindings::reflector::reflect_dom_object;
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::DOMString;
use crate::dom::bindings::trace::JSTraceable;
use crate::dom::bindings::weakref::{WeakRef, WeakRefVec};
use crate::dom::characterdata::CharacterData;
use crate::dom::document::Document;
use crate::dom::documentfragment::DocumentFragment;
use crate::dom::element::Element;
use crate::dom::htmlscriptelement::HTMLScriptElement;
use crate::dom::node::{Node, ShadowIncluding, UnbindContext};
use crate::dom::text::Text;
use crate::dom::window::Window;
use dom_struct::dom_struct;
use js::jsapi::JSTracer;
use malloc_size_of::{MallocSizeOf, MallocSizeOfOps};
use std::cell::UnsafeCell;
use std::cmp::{Ordering, PartialOrd};

#[dom_struct]
pub struct Range {
    abstractrange: AbstractRange,
}

impl Range {
    fn new_inherited(
        start_container: &Node,
        start_offset: u32,
        end_container: &Node,
        end_offset: u32,
    ) -> Range {
        debug_assert!(start_offset <= start_container.len());
        debug_assert!(end_offset <= end_container.len());
        Range {
            abstractrange: AbstractRange::new_inherited(
                start_container,
                start_offset,
                end_container,
                end_offset,
            ),
        }
    }

    pub fn new_with_doc(document: &Document) -> DomRoot<Range> {
        let root = document.upcast();
        Range::new(document, root, 0, root, 0)
    }

    pub fn new(
        document: &Document,
        start_container: &Node,
        start_offset: u32,
        end_container: &Node,
        end_offset: u32,
    ) -> DomRoot<Range> {
        let range = reflect_dom_object(
            Box::new(Range::new_inherited(
                start_container,
                start_offset,
                end_container,
                end_offset,
            )),
            document.window(),
            RangeBinding::Wrap,
        );
        start_container.ranges().push(WeakRef::new(&range));
        if start_container != end_container {
            end_container.ranges().push(WeakRef::new(&range));
        }
        range
    }

    // https://dom.spec.whatwg.org/#dom-range
    pub fn Constructor(window: &Window) -> Fallible<DomRoot<Range>> {
        let document = window.Document();
        Ok(Range::new_with_doc(&document))
    }

    // https://dom.spec.whatwg.org/#contained
    fn contains(&self, node: &Node) -> bool {
        match (
            bp_position(
                node,
                0,
                &self.abstractrange.StartContainer(),
                self.abstractrange.StartOffset(),
            ),
            bp_position(
                node,
                node.len(),
                &self.abstractrange.EndContainer(),
                self.abstractrange.EndOffset(),
            ),
        ) {
            (Some(Ordering::Greater), Some(Ordering::Less)) => true,
            _ => false,
        }
    }

    // https://dom.spec.whatwg.org/#partially-contained
    fn partially_contains(&self, node: &Node) -> bool {
        self.abstractrange
            .StartContainer()
            .inclusive_ancestors(ShadowIncluding::No)
            .any(|n| &*n == node) !=
            self.abstractrange
                .EndContainer()
                .inclusive_ancestors(ShadowIncluding::No)
                .any(|n| &*n == node)
    }

    // https://dom.spec.whatwg.org/#concept-range-clone
    fn contained_children(
        &self,
    ) -> Fallible<(
        Option<DomRoot<Node>>,
        Option<DomRoot<Node>>,
        Vec<DomRoot<Node>>,
    )> {
        let start_node = self.abstractrange.StartContainer();
        let end_node = self.abstractrange.EndContainer();
        // Steps 5-6.
        let common_ancestor = self.CommonAncestorContainer();

        let first_contained_child = if start_node.is_inclusive_ancestor_of(&end_node) {
            // Step 7.
            None
        } else {
            // Step 8.
            common_ancestor
                .children()
                .find(|node| Range::partially_contains(self, node))
        };

        let last_contained_child = if end_node.is_inclusive_ancestor_of(&start_node) {
            // Step 9.
            None
        } else {
            // Step 10.
            common_ancestor
                .rev_children()
                .find(|node| Range::partially_contains(self, node))
        };

        // Step 11.
        let contained_children: Vec<DomRoot<Node>> = common_ancestor
            .children()
            .filter(|n| self.contains(n))
            .collect();

        // Step 12.
        if contained_children.iter().any(|n| n.is_doctype()) {
            return Err(Error::HierarchyRequest);
        }

        Ok((
            first_contained_child,
            last_contained_child,
            contained_children,
        ))
    }

    // https://dom.spec.whatwg.org/#concept-range-bp-set
    fn set_start(&self, node: &Node, offset: u32) {
        if self.abstractrange.start().node() != node {
            if self.abstractrange.start().node() == self.abstractrange.end().node() {
                node.ranges().push(WeakRef::new(&self));
            } else if self.abstractrange.end().node() == node {
                self.abstractrange.StartContainer().ranges().remove(self);
            } else {
                node.ranges()
                    .push(self.abstractrange.StartContainer().ranges().remove(self));
            }
        }
        self.abstractrange.start().set(node, offset);
    }

    // https://dom.spec.whatwg.org/#concept-range-bp-set
    fn set_end(&self, node: &Node, offset: u32) {
        if self.abstractrange.end().node() != node {
            if self.abstractrange.end().node() == self.abstractrange.start().node() {
                node.ranges().push(WeakRef::new(&self));
            } else if self.abstractrange.start().node() == node {
                self.abstractrange.EndContainer().ranges().remove(self);
            } else {
                node.ranges()
                    .push(self.abstractrange.EndContainer().ranges().remove(self));
            }
        }
        self.abstractrange.end().set(node, offset);
    }

    // https://dom.spec.whatwg.org/#dom-range-comparepointnode-offset
    fn compare_point(&self, node: &Node, offset: u32) -> Fallible<Ordering> {
        let start_node = self.abstractrange.StartContainer();
        let start_node_root = start_node
            .inclusive_ancestors(ShadowIncluding::No)
            .last()
            .unwrap();
        let node_root = node
            .inclusive_ancestors(ShadowIncluding::No)
            .last()
            .unwrap();
        if start_node_root != node_root {
            // Step 1.
            return Err(Error::WrongDocument);
        }
        if node.is_doctype() {
            // Step 2.
            return Err(Error::InvalidNodeType);
        }
        if offset > node.len() {
            // Step 3.
            return Err(Error::IndexSize);
        }
        if let Ordering::Less =
            bp_position(node, offset, &start_node, self.abstractrange.StartOffset()).unwrap()
        {
            // Step 4.
            return Ok(Ordering::Less);
        }
        if let Ordering::Greater = bp_position(
            node,
            offset,
            &self.abstractrange.EndContainer(),
            self.abstractrange.EndOffset(),
        )
        .unwrap()
        {
            // Step 5.
            return Ok(Ordering::Greater);
        }
        // Step 6.
        Ok(Ordering::Equal)
    }
}

impl RangeMethods for Range {
    // https://dom.spec.whatwg.org/#dom-range-commonancestorcontainer
    fn CommonAncestorContainer(&self) -> DomRoot<Node> {
        let end_container = self.abstractrange.EndContainer();
        // Step 1.
        for container in self
            .abstractrange
            .StartContainer()
            .inclusive_ancestors(ShadowIncluding::No)
        {
            // Step 2.
            if container.is_inclusive_ancestor_of(&end_container) {
                // Step 3.
                return container;
            }
        }
        unreachable!();
    }

    // https://dom.spec.whatwg.org/#dom-range-setstart
    fn SetStart(&self, node: &Node, offset: u32) -> ErrorResult {
        if node.is_doctype() {
            // Step 1.
            Err(Error::InvalidNodeType)
        } else if offset > node.len() {
            // Step 2.
            Err(Error::IndexSize)
        } else {
            // Step 3.
            self.set_start(node, offset);
            if !(self.abstractrange.start() <= self.abstractrange.end()) {
                // Step 4.
                self.set_end(node, offset);
            }
            Ok(())
        }
    }

    // https://dom.spec.whatwg.org/#dom-range-setend
    fn SetEnd(&self, node: &Node, offset: u32) -> ErrorResult {
        if node.is_doctype() {
            // Step 1.
            Err(Error::InvalidNodeType)
        } else if offset > node.len() {
            // Step 2.
            Err(Error::IndexSize)
        } else {
            // Step 3.
            self.set_end(node, offset);
            if !(self.abstractrange.end() >= self.abstractrange.start()) {
                // Step 4.
                self.set_start(node, offset);
            }
            Ok(())
        }
    }

    // https://dom.spec.whatwg.org/#dom-range-setstartbefore
    fn SetStartBefore(&self, node: &Node) -> ErrorResult {
        let parent = node.GetParentNode().ok_or(Error::InvalidNodeType)?;
        self.SetStart(&parent, node.index())
    }

    // https://dom.spec.whatwg.org/#dom-range-setstartafter
    fn SetStartAfter(&self, node: &Node) -> ErrorResult {
        let parent = node.GetParentNode().ok_or(Error::InvalidNodeType)?;
        self.SetStart(&parent, node.index() + 1)
    }

    // https://dom.spec.whatwg.org/#dom-range-setendbefore
    fn SetEndBefore(&self, node: &Node) -> ErrorResult {
        let parent = node.GetParentNode().ok_or(Error::InvalidNodeType)?;
        self.SetEnd(&parent, node.index())
    }

    // https://dom.spec.whatwg.org/#dom-range-setendafter
    fn SetEndAfter(&self, node: &Node) -> ErrorResult {
        let parent = node.GetParentNode().ok_or(Error::InvalidNodeType)?;
        self.SetEnd(&parent, node.index() + 1)
    }

    // https://dom.spec.whatwg.org/#dom-range-collapse
    fn Collapse(&self, to_start: bool) {
        if to_start {
            self.set_end(
                &self.abstractrange.StartContainer(),
                self.abstractrange.StartOffset(),
            );
        } else {
            self.set_start(
                &self.abstractrange.EndContainer(),
                self.abstractrange.EndOffset(),
            );
        }
    }

    // https://dom.spec.whatwg.org/#dom-range-selectnode
    fn SelectNode(&self, node: &Node) -> ErrorResult {
        // Steps 1, 2.
        let parent = node.GetParentNode().ok_or(Error::InvalidNodeType)?;
        // Step 3.
        let index = node.index();
        // Step 4.
        self.set_start(&parent, index);
        // Step 5.
        self.set_end(&parent, index + 1);
        Ok(())
    }

    // https://dom.spec.whatwg.org/#dom-range-selectnodecontents
    fn SelectNodeContents(&self, node: &Node) -> ErrorResult {
        if node.is_doctype() {
            // Step 1.
            return Err(Error::InvalidNodeType);
        }
        // Step 2.
        let length = node.len();
        // Step 3.
        self.set_start(node, 0);
        // Step 4.
        self.set_end(node, length);
        Ok(())
    }

    // https://dom.spec.whatwg.org/#dom-range-compareboundarypoints
    fn CompareBoundaryPoints(&self, how: u16, other: &Range) -> Fallible<i16> {
        if how > RangeConstants::END_TO_START {
            // Step 1.
            return Err(Error::NotSupported);
        }
        let this_root = self
            .abstractrange
            .StartContainer()
            .inclusive_ancestors(ShadowIncluding::No)
            .last()
            .unwrap();
        let other_root = other
            .abstractrange
            .StartContainer()
            .inclusive_ancestors(ShadowIncluding::No)
            .last()
            .unwrap();
        if this_root != other_root {
            // Step 2.
            return Err(Error::WrongDocument);
        }
        // Step 3.
        let (this_point, other_point) = match how {
            RangeConstants::START_TO_START => {
                (self.abstractrange.start(), self.abstractrange.start())
            },
            RangeConstants::START_TO_END => (self.abstractrange.end(), self.abstractrange.start()),
            RangeConstants::END_TO_END => (self.abstractrange.end(), self.abstractrange.end()),
            RangeConstants::END_TO_START => (self.abstractrange.start(), self.abstractrange.end()),
            _ => unreachable!(),
        };
        // step 4.
        match this_point.partial_cmp(other_point).unwrap() {
            Ordering::Less => Ok(-1),
            Ordering::Equal => Ok(0),
            Ordering::Greater => Ok(1),
        }
    }

    // https://dom.spec.whatwg.org/#dom-range-clonerange
    fn CloneRange(&self) -> DomRoot<Range> {
        let start_node = self.abstractrange.StartContainer();
        let owner_doc = start_node.owner_doc();
        Range::new(
            &owner_doc,
            &start_node,
            self.abstractrange.StartOffset(),
            &self.abstractrange.EndContainer(),
            self.abstractrange.EndOffset(),
        )
    }

    // https://dom.spec.whatwg.org/#dom-range-ispointinrange
    fn IsPointInRange(&self, node: &Node, offset: u32) -> Fallible<bool> {
        match self.compare_point(node, offset) {
            Ok(Ordering::Less) => Ok(false),
            Ok(Ordering::Equal) => Ok(true),
            Ok(Ordering::Greater) => Ok(false),
            Err(Error::WrongDocument) => {
                // Step 2.
                Ok(false)
            },
            Err(error) => Err(error),
        }
    }

    // https://dom.spec.whatwg.org/#dom-range-comparepoint
    fn ComparePoint(&self, node: &Node, offset: u32) -> Fallible<i16> {
        self.compare_point(node, offset).map(|order| match order {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        })
    }

    // https://dom.spec.whatwg.org/#dom-range-intersectsnode
    fn IntersectsNode(&self, node: &Node) -> bool {
        let start_node = self.abstractrange.StartContainer();
        let start_node_root = self
            .abstractrange
            .StartContainer()
            .inclusive_ancestors(ShadowIncluding::No)
            .last()
            .unwrap();
        let node_root = node
            .inclusive_ancestors(ShadowIncluding::No)
            .last()
            .unwrap();
        if start_node_root != node_root {
            // Step 1.
            return false;
        }
        let parent = match node.GetParentNode() {
            Some(parent) => parent,
            None => {
                // Step 3.
                return true;
            },
        };
        // Step 4.
        let offset = node.index();
        // Step 5.
        Ordering::Greater ==
            bp_position(
                &parent,
                offset + 1,
                &start_node,
                self.abstractrange.StartOffset(),
            )
            .unwrap() &&
            Ordering::Less ==
                bp_position(
                    &parent,
                    offset,
                    &self.abstractrange.EndContainer(),
                    self.abstractrange.EndOffset(),
                )
                .unwrap()
    }

    // https://dom.spec.whatwg.org/#dom-range-clonecontents
    // https://dom.spec.whatwg.org/#concept-range-clone
    fn CloneContents(&self) -> Fallible<DomRoot<DocumentFragment>> {
        // Step 3.
        let start_node = self.abstractrange.StartContainer();
        let start_offset = self.abstractrange.StartOffset();
        let end_node = self.abstractrange.EndContainer();
        let end_offset = self.abstractrange.EndOffset();

        // Step 1.
        let fragment = DocumentFragment::new(&start_node.owner_doc());

        // Step 2.
        if self.abstractrange.start() == self.abstractrange.end() {
            return Ok(fragment);
        }

        if end_node == start_node {
            if let Some(cdata) = start_node.downcast::<CharacterData>() {
                // Steps 4.1-2.
                let data = cdata
                    .SubstringData(start_offset, end_offset - start_offset)
                    .unwrap();
                let clone = cdata.clone_with_data(data, &start_node.owner_doc());
                // Step 4.3.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 4.4
                return Ok(fragment);
            }
        }

        // Steps 5-12.
        let (first_contained_child, last_contained_child, contained_children) =
            self.contained_children()?;

        if let Some(child) = first_contained_child {
            // Step 13.
            if let Some(cdata) = child.downcast::<CharacterData>() {
                assert!(child == start_node);
                // Steps 13.1-2.
                let data = cdata
                    .SubstringData(start_offset, start_node.len() - start_offset)
                    .unwrap();
                let clone = cdata.clone_with_data(data, &start_node.owner_doc());
                // Step 13.3.
                fragment.upcast::<Node>().AppendChild(&clone)?;
            } else {
                // Step 14.1.
                let clone = child.CloneNode(/* deep */ false)?;
                // Step 14.2.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 14.3.
                let subrange = Range::new(
                    &clone.owner_doc(),
                    &start_node,
                    start_offset,
                    &child,
                    child.len(),
                );
                // Step 14.4.
                let subfragment = subrange.CloneContents()?;
                // Step 14.5.
                clone.AppendChild(subfragment.upcast())?;
            }
        }

        // Step 15.
        for child in contained_children {
            // Step 15.1.
            let clone = child.CloneNode(/* deep */ true)?;
            // Step 15.2.
            fragment.upcast::<Node>().AppendChild(&clone)?;
        }

        if let Some(child) = last_contained_child {
            // Step 16.
            if let Some(cdata) = child.downcast::<CharacterData>() {
                assert!(child == end_node);
                // Steps 16.1-2.
                let data = cdata.SubstringData(0, end_offset).unwrap();
                let clone = cdata.clone_with_data(data, &start_node.owner_doc());
                // Step 16.3.
                fragment.upcast::<Node>().AppendChild(&clone)?;
            } else {
                // Step 17.1.
                let clone = child.CloneNode(/* deep */ false)?;
                // Step 17.2.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 17.3.
                let subrange = Range::new(&clone.owner_doc(), &child, 0, &end_node, end_offset);
                // Step 17.4.
                let subfragment = subrange.CloneContents()?;
                // Step 17.5.
                clone.AppendChild(subfragment.upcast())?;
            }
        }

        // Step 18.
        Ok(fragment)
    }

    // https://dom.spec.whatwg.org/#dom-range-extractcontents
    // https://dom.spec.whatwg.org/#concept-range-extract
    fn ExtractContents(&self) -> Fallible<DomRoot<DocumentFragment>> {
        // Step 3.
        let start_node = self.abstractrange.StartContainer();
        let start_offset = self.abstractrange.StartOffset();
        let end_node = self.abstractrange.EndContainer();
        let end_offset = self.abstractrange.EndOffset();

        // Step 1.
        let fragment = DocumentFragment::new(&start_node.owner_doc());

        // Step 2.
        if self.abstractrange.Collapsed() {
            return Ok(fragment);
        }

        if end_node == start_node {
            if let Some(end_data) = end_node.downcast::<CharacterData>() {
                // Step 4.1.
                let clone = end_node.CloneNode(/* deep */ true)?;
                // Step 4.2.
                let text = end_data.SubstringData(start_offset, end_offset - start_offset);
                clone
                    .downcast::<CharacterData>()
                    .unwrap()
                    .SetData(text.unwrap());
                // Step 4.3.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 4.4.
                end_data.ReplaceData(start_offset, end_offset - start_offset, DOMString::new())?;
                // Step 4.5.
                return Ok(fragment);
            }
        }

        // Steps 5-12.
        let (first_contained_child, last_contained_child, contained_children) =
            self.contained_children()?;

        let (new_node, new_offset) = if start_node.is_inclusive_ancestor_of(&end_node) {
            // Step 13.
            (DomRoot::from_ref(&*start_node), start_offset)
        } else {
            // Step 14.1-2.
            let reference_node = start_node
                .ancestors()
                .take_while(|n| !n.is_inclusive_ancestor_of(&end_node))
                .last()
                .unwrap_or(DomRoot::from_ref(&start_node));
            // Step 14.3.
            (
                reference_node.GetParentNode().unwrap(),
                reference_node.index() + 1,
            )
        };

        if let Some(child) = first_contained_child {
            if let Some(start_data) = child.downcast::<CharacterData>() {
                assert!(child == start_node);
                // Step 15.1.
                let clone = start_node.CloneNode(/* deep */ true)?;
                // Step 15.2.
                let text = start_data.SubstringData(start_offset, start_node.len() - start_offset);
                clone
                    .downcast::<CharacterData>()
                    .unwrap()
                    .SetData(text.unwrap());
                // Step 15.3.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 15.4.
                start_data.ReplaceData(
                    start_offset,
                    start_node.len() - start_offset,
                    DOMString::new(),
                )?;
            } else {
                // Step 16.1.
                let clone = child.CloneNode(/* deep */ false)?;
                // Step 16.2.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 16.3.
                let subrange = Range::new(
                    &clone.owner_doc(),
                    &start_node,
                    start_offset,
                    &child,
                    child.len(),
                );
                // Step 16.4.
                let subfragment = subrange.ExtractContents()?;
                // Step 16.5.
                clone.AppendChild(subfragment.upcast())?;
            }
        }

        // Step 17.
        for child in contained_children {
            fragment.upcast::<Node>().AppendChild(&child)?;
        }

        if let Some(child) = last_contained_child {
            if let Some(end_data) = child.downcast::<CharacterData>() {
                assert!(child == end_node);
                // Step 18.1.
                let clone = end_node.CloneNode(/* deep */ true)?;
                // Step 18.2.
                let text = end_data.SubstringData(0, end_offset);
                clone
                    .downcast::<CharacterData>()
                    .unwrap()
                    .SetData(text.unwrap());
                // Step 18.3.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 18.4.
                end_data.ReplaceData(0, end_offset, DOMString::new())?;
            } else {
                // Step 19.1.
                let clone = child.CloneNode(/* deep */ false)?;
                // Step 19.2.
                fragment.upcast::<Node>().AppendChild(&clone)?;
                // Step 19.3.
                let subrange = Range::new(&clone.owner_doc(), &child, 0, &end_node, end_offset);
                // Step 19.4.
                let subfragment = subrange.ExtractContents()?;
                // Step 19.5.
                clone.AppendChild(subfragment.upcast())?;
            }
        }

        // Step 20.
        self.SetStart(&new_node, new_offset)?;
        self.SetEnd(&new_node, new_offset)?;

        // Step 21.
        Ok(fragment)
    }

    // https://dom.spec.whatwg.org/#dom-range-detach
    fn Detach(&self) {
        // This method intentionally left blank.
    }

    // https://dom.spec.whatwg.org/#dom-range-insertnode
    // https://dom.spec.whatwg.org/#concept-range-insert
    fn InsertNode(&self, node: &Node) -> ErrorResult {
        let start_node = self.abstractrange.StartContainer();
        let start_offset = self.abstractrange.StartOffset();

        // Step 1.
        if &*start_node == node {
            return Err(Error::HierarchyRequest);
        }
        match start_node.type_id() {
            // Handled under step 2.
            NodeTypeId::CharacterData(CharacterDataTypeId::Text(_)) => (),
            NodeTypeId::CharacterData(_) => return Err(Error::HierarchyRequest),
            _ => (),
        }

        // Step 2.
        let (reference_node, parent) = match start_node.type_id() {
            NodeTypeId::CharacterData(CharacterDataTypeId::Text(_)) => {
                // Step 3.
                let parent = match start_node.GetParentNode() {
                    Some(parent) => parent,
                    // Step 1.
                    None => return Err(Error::HierarchyRequest),
                };
                // Step 5.
                (Some(DomRoot::from_ref(&*start_node)), parent)
            },
            _ => {
                // Steps 4-5.
                let child = start_node.ChildNodes().Item(start_offset);
                (child, DomRoot::from_ref(&*start_node))
            },
        };

        // Step 6.
        Node::ensure_pre_insertion_validity(node, &parent, reference_node.as_deref())?;

        // Step 7.
        let split_text;
        let reference_node = match start_node.downcast::<Text>() {
            Some(text) => {
                split_text = text.SplitText(start_offset)?;
                let new_reference = DomRoot::upcast::<Node>(split_text);
                assert!(new_reference.GetParentNode().as_deref() == Some(&parent));
                Some(new_reference)
            },
            _ => reference_node,
        };

        // Step 8.
        let reference_node = if Some(node) == reference_node.as_deref() {
            node.GetNextSibling()
        } else {
            reference_node
        };

        // Step 9.
        node.remove_self();

        // Step 10.
        let new_offset = reference_node
            .as_ref()
            .map_or(parent.len(), |node| node.index());

        // Step 11
        let new_offset = new_offset +
            if let NodeTypeId::DocumentFragment(_) = node.type_id() {
                node.len()
            } else {
                1
            };

        // Step 12.
        Node::pre_insert(node, &parent, reference_node.as_deref())?;

        // Step 13.
        if self.abstractrange.Collapsed() {
            self.set_end(&parent, new_offset);
        }

        Ok(())
    }

    // https://dom.spec.whatwg.org/#dom-range-deletecontents
    fn DeleteContents(&self) -> ErrorResult {
        // Step 1.
        if self.abstractrange.Collapsed() {
            return Ok(());
        }

        // Step 2.
        let start_node = self.abstractrange.StartContainer();
        let end_node = self.abstractrange.EndContainer();
        let start_offset = self.abstractrange.StartOffset();
        let end_offset = self.abstractrange.EndOffset();

        // Step 3.
        if start_node == end_node {
            if let Some(text) = start_node.downcast::<CharacterData>() {
                return text.ReplaceData(start_offset, end_offset - start_offset, DOMString::new());
            }
        }

        // Step 4.
        rooted_vec!(let mut contained_children);
        let ancestor = self.CommonAncestorContainer();

        let mut iter = start_node.following_nodes(&ancestor);

        let mut next = iter.next();
        while let Some(child) = next {
            if self.contains(&child) {
                contained_children.push(Dom::from_ref(&*child));
                next = iter.next_skipping_children();
            } else {
                next = iter.next();
            }
        }

        let (new_node, new_offset) = if start_node.is_inclusive_ancestor_of(&end_node) {
            // Step 5.
            (DomRoot::from_ref(&*start_node), start_offset)
        } else {
            // Step 6.
            fn compute_reference(start_node: &Node, end_node: &Node) -> (DomRoot<Node>, u32) {
                let mut reference_node = DomRoot::from_ref(start_node);
                while let Some(parent) = reference_node.GetParentNode() {
                    if parent.is_inclusive_ancestor_of(end_node) {
                        return (parent, reference_node.index() + 1);
                    }
                    reference_node = parent;
                }
                unreachable!()
            }

            compute_reference(&start_node, &end_node)
        };

        // Step 7.
        if let Some(text) = start_node.downcast::<CharacterData>() {
            text.ReplaceData(
                start_offset,
                start_node.len() - start_offset,
                DOMString::new(),
            )
            .unwrap();
        }

        // Step 8.
        for child in &*contained_children {
            child.remove_self();
        }

        // Step 9.
        if let Some(text) = end_node.downcast::<CharacterData>() {
            text.ReplaceData(0, end_offset, DOMString::new()).unwrap();
        }

        // Step 10.
        self.SetStart(&new_node, new_offset).unwrap();
        self.SetEnd(&new_node, new_offset).unwrap();
        Ok(())
    }

    // https://dom.spec.whatwg.org/#dom-range-surroundcontents
    fn SurroundContents(&self, new_parent: &Node) -> ErrorResult {
        // Step 1.
        let start = self.abstractrange.StartContainer();
        let end = self.abstractrange.EndContainer();

        if start
            .inclusive_ancestors(ShadowIncluding::No)
            .any(|n| !n.is_inclusive_ancestor_of(&end) && !n.is::<Text>()) ||
            end.inclusive_ancestors(ShadowIncluding::No)
                .any(|n| !n.is_inclusive_ancestor_of(&start) && !n.is::<Text>())
        {
            return Err(Error::InvalidState);
        }

        // Step 2.
        match new_parent.type_id() {
            NodeTypeId::Document(_) |
            NodeTypeId::DocumentType |
            NodeTypeId::DocumentFragment(_) => {
                return Err(Error::InvalidNodeType);
            },
            _ => (),
        }

        // Step 3.
        let fragment = self.ExtractContents()?;

        // Step 4.
        Node::replace_all(None, new_parent);

        // Step 5.
        self.InsertNode(new_parent)?;

        // Step 6.
        new_parent.AppendChild(fragment.upcast())?;

        // Step 7.
        self.SelectNode(new_parent)
    }

    // https://dom.spec.whatwg.org/#dom-range-stringifier
    fn Stringifier(&self) -> DOMString {
        let start_node = self.abstractrange.StartContainer();
        let end_node = self.abstractrange.EndContainer();

        // Step 1.
        let mut s = DOMString::new();

        if let Some(text_node) = start_node.downcast::<Text>() {
            let char_data = text_node.upcast::<CharacterData>();

            // Step 2.
            if start_node == end_node {
                return char_data
                    .SubstringData(
                        self.abstractrange.StartOffset(),
                        self.abstractrange.EndOffset() - self.abstractrange.StartOffset(),
                    )
                    .unwrap();
            }

            // Step 3.
            s.push_str(
                &*char_data
                    .SubstringData(
                        self.abstractrange.StartOffset(),
                        char_data.Length() - self.abstractrange.StartOffset(),
                    )
                    .unwrap(),
            );
        }

        // Step 4.
        let ancestor = self.CommonAncestorContainer();
        let mut iter = start_node
            .following_nodes(&ancestor)
            .filter_map(DomRoot::downcast::<Text>);

        while let Some(child) = iter.next() {
            if self.contains(child.upcast()) {
                s.push_str(&*child.upcast::<CharacterData>().Data());
            }
        }

        // Step 5.
        if let Some(text_node) = end_node.downcast::<Text>() {
            let char_data = text_node.upcast::<CharacterData>();
            s.push_str(
                &*char_data
                    .SubstringData(0, self.abstractrange.EndOffset())
                    .unwrap(),
            );
        }

        // Step 6.
        s
    }

    // https://dvcs.w3.org/hg/innerhtml/raw-file/tip/index.html#extensions-to-the-range-interface
    fn CreateContextualFragment(&self, fragment: DOMString) -> Fallible<DomRoot<DocumentFragment>> {
        // Step 1.
        let node = self.abstractrange.StartContainer();
        let owner_doc = node.owner_doc();
        let element = match node.type_id() {
            NodeTypeId::Document(_) | NodeTypeId::DocumentFragment(_) => None,
            NodeTypeId::Element(_) => Some(DomRoot::downcast::<Element>(node).unwrap()),
            NodeTypeId::CharacterData(CharacterDataTypeId::Comment) |
            NodeTypeId::CharacterData(CharacterDataTypeId::Text(_)) => node.GetParentElement(),
            NodeTypeId::CharacterData(CharacterDataTypeId::ProcessingInstruction) |
            NodeTypeId::DocumentType => unreachable!(),
        };

        // Step 2.
        let element = Element::fragment_parsing_context(&owner_doc, element.as_deref());

        // Step 3.
        let fragment_node = element.parse_fragment(fragment)?;

        // Step 4.
        for node in fragment_node
            .upcast::<Node>()
            .traverse_preorder(ShadowIncluding::No)
        {
            if let Some(script) = node.downcast::<HTMLScriptElement>() {
                script.set_already_started(false);
                script.set_parser_inserted(false);
            }
        }

        // Step 5.
        Ok(fragment_node)
    }
}

pub struct WeakRangeVec {
    cell: UnsafeCell<WeakRefVec<Range>>,
}

#[allow(unsafe_code)]
impl WeakRangeVec {
    /// Create a new vector of weak references.
    pub fn new() -> Self {
        WeakRangeVec {
            cell: UnsafeCell::new(WeakRefVec::new()),
        }
    }

    /// Whether that vector of ranges is empty.
    pub fn is_empty(&self) -> bool {
        unsafe { (*self.cell.get()).is_empty() }
    }

    /// Used for steps 2.1-2. when inserting a node.
    /// <https://dom.spec.whatwg.org/#concept-node-insert>
    pub fn increase_above(&self, node: &Node, offset: u32, delta: u32) {
        self.map_offset_above(node, offset, |offset| offset + delta);
    }

    /// Used for steps 4-5. when removing a node.
    /// <https://dom.spec.whatwg.org/#concept-node-remove>
    pub fn decrease_above(&self, node: &Node, offset: u32, delta: u32) {
        self.map_offset_above(node, offset, |offset| offset - delta);
    }

    /// Used for steps 2-3. when removing a node.
    /// <https://dom.spec.whatwg.org/#concept-node-remove>
    pub fn drain_to_parent(&self, context: &UnbindContext, child: &Node) {
        if self.is_empty() {
            return;
        }

        let offset = context.index();
        let parent = context.parent;
        unsafe {
            let ranges = &mut *self.cell.get();

            ranges.update(|entry| {
                let range = entry.root().unwrap();
                if range.abstractrange.start().node() == parent ||
                    range.abstractrange.end().node() == parent
                {
                    entry.remove();
                }
                if range.abstractrange.start().node() == child {
                    range.abstractrange.start().set(context.parent, offset);
                }
                if range.abstractrange.end().node() == child {
                    range.abstractrange.end().set(context.parent, offset);
                }
            });

            (*context.parent.ranges().cell.get()).extend(ranges.drain(..));
        }
    }

    /// Used for steps 7.1-2. when normalizing a node.
    /// <https://dom.spec.whatwg.org/#dom-node-normalize>
    pub fn drain_to_preceding_text_sibling(&self, node: &Node, sibling: &Node, length: u32) {
        if self.is_empty() {
            return;
        }

        unsafe {
            let ranges = &mut *self.cell.get();

            ranges.update(|entry| {
                let range = entry.root().unwrap();
                if range.abstractrange.start().node() == sibling ||
                    range.abstractrange.end().node() == sibling
                {
                    entry.remove();
                }
                if range.abstractrange.start().node() == node {
                    range
                        .abstractrange
                        .start()
                        .set(sibling, range.abstractrange.StartOffset() + length);
                }
                if range.abstractrange.end().node() == node {
                    range
                        .abstractrange
                        .end()
                        .set(sibling, range.abstractrange.EndOffset() + length);
                }
            });

            (*sibling.ranges().cell.get()).extend(ranges.drain(..));
        }
    }

    /// Used for steps 7.3-4. when normalizing a node.
    /// <https://dom.spec.whatwg.org/#dom-node-normalize>
    pub fn move_to_text_child_at(&self, node: &Node, offset: u32, child: &Node, new_offset: u32) {
        unsafe {
            let child_ranges = &mut *child.ranges().cell.get();

            (*self.cell.get()).update(|entry| {
                let range = entry.root().unwrap();

                let node_is_start = range.abstractrange.start().node() == node;
                let node_is_end = range.abstractrange.end().node() == node;

                let move_start = node_is_start && range.abstractrange.StartOffset() == offset;
                let move_end = node_is_end && range.abstractrange.EndOffset() == offset;

                let remove_from_node = move_start && move_end ||
                    move_start && !node_is_end ||
                    move_end && !node_is_start;

                let already_in_child = range.abstractrange.start().node() == child ||
                    range.abstractrange.end().node() == child;
                let push_to_child = !already_in_child && (move_start || move_end);

                if remove_from_node {
                    let ref_ = entry.remove();
                    if push_to_child {
                        child_ranges.push(ref_);
                    }
                } else if push_to_child {
                    child_ranges.push(WeakRef::new(&range));
                }

                if move_start {
                    range.abstractrange.start().set(child, new_offset);
                }
                if move_end {
                    range.abstractrange.end().set(child, new_offset);
                }
            });
        }
    }

    /// Used for steps 8-11. when replacing character data.
    /// <https://dom.spec.whatwg.org/#concept-cd-replace>
    pub fn replace_code_units(
        &self,
        node: &Node,
        offset: u32,
        removed_code_units: u32,
        added_code_units: u32,
    ) {
        self.map_offset_above(node, offset, |range_offset| {
            if range_offset <= offset + removed_code_units {
                offset
            } else {
                range_offset + added_code_units - removed_code_units
            }
        });
    }

    /// Used for steps 7.2-3. when splitting a text node.
    /// <https://dom.spec.whatwg.org/#concept-text-split>
    pub fn move_to_following_text_sibling_above(&self, node: &Node, offset: u32, sibling: &Node) {
        unsafe {
            let sibling_ranges = &mut *sibling.ranges().cell.get();

            (*self.cell.get()).update(|entry| {
                let range = entry.root().unwrap();
                let start_offset = range.abstractrange.StartOffset();
                let end_offset = range.abstractrange.EndOffset();

                let node_is_start = range.abstractrange.start().node() == node;
                let node_is_end = range.abstractrange.end().node() == node;

                let move_start = node_is_start && start_offset > offset;
                let move_end = node_is_end && end_offset > offset;

                let remove_from_node = move_start && move_end ||
                    move_start && !node_is_end ||
                    move_end && !node_is_start;

                let already_in_sibling = range.abstractrange.start().node() == sibling ||
                    range.abstractrange.end().node() == sibling;
                let push_to_sibling = !already_in_sibling && (move_start || move_end);

                if remove_from_node {
                    let ref_ = entry.remove();
                    if push_to_sibling {
                        sibling_ranges.push(ref_);
                    }
                } else if push_to_sibling {
                    sibling_ranges.push(WeakRef::new(&range));
                }

                if move_start {
                    range
                        .abstractrange
                        .start()
                        .set(sibling, start_offset - offset);
                }
                if move_end {
                    range.abstractrange.end().set(sibling, end_offset - offset);
                }
            });
        }
    }

    /// Used for steps 7.4-5. when splitting a text node.
    /// <https://dom.spec.whatwg.org/#concept-text-split>
    pub fn increment_at(&self, node: &Node, offset: u32) {
        unsafe {
            (*self.cell.get()).update(|entry| {
                let range = entry.root().unwrap();
                if range.abstractrange.start().node() == node &&
                    offset == range.abstractrange.StartOffset()
                {
                    range.abstractrange.start().set_offset(offset + 1);
                }
                if range.abstractrange.end().node() == node &&
                    offset == range.abstractrange.EndOffset()
                {
                    range.abstractrange.end().set_offset(offset + 1);
                }
            });
        }
    }

    fn map_offset_above<F: FnMut(u32) -> u32>(&self, node: &Node, offset: u32, mut f: F) {
        unsafe {
            (*self.cell.get()).update(|entry| {
                let range = entry.root().unwrap();
                let start_offset = range.abstractrange.StartOffset();
                if range.abstractrange.start().node() == node && start_offset > offset {
                    range.abstractrange.start().set_offset(f(start_offset));
                }
                let end_offset = range.abstractrange.EndOffset();
                if range.abstractrange.end().node() == node && end_offset > offset {
                    range.abstractrange.end().set_offset(f(end_offset));
                }
            });
        }
    }

    pub fn push(&self, ref_: WeakRef<Range>) {
        unsafe {
            (*self.cell.get()).push(ref_);
        }
    }

    fn remove(&self, range: &Range) -> WeakRef<Range> {
        unsafe {
            let ranges = &mut *self.cell.get();
            let position = ranges.iter().position(|ref_| ref_ == range).unwrap();
            ranges.swap_remove(position)
        }
    }
}

#[allow(unsafe_code)]
impl MallocSizeOf for WeakRangeVec {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        unsafe { (*self.cell.get()).size_of(ops) }
    }
}

#[allow(unsafe_code)]
unsafe impl JSTraceable for WeakRangeVec {
    unsafe fn trace(&self, _: *mut JSTracer) {
        (*self.cell.get()).retain_alive()
    }
}
